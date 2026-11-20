//! Startup negotiation on a socket after applying the DSN's TLS policy.

use std::collections::BTreeMap;
use std::io;

use postgres_protocol::authentication::md5_hash;
use postgres_protocol::authentication::sasl::{ChannelBinding, SCRAM_SHA_256, ScramSha256};
use postgres_protocol::message::{backend::Message, frontend};

use super::{MAX_MESSAGE, Transport, invalid};
use crate::errors::DbError;

#[derive(Debug)]
pub(in super::super) enum StartError {
    Transport(io::Error),
    Server(DbError),
}

impl From<io::Error> for StartError {
    fn from(error: io::Error) -> Self {
        Self::Transport(error)
    }
}

pub(in super::super) struct Session {
    pub parameters: BTreeMap<String, String>,
    pub pid: i32,
    pub status: u8,
}

pub(in super::super) async fn start(
    transport: &mut Transport,
    parameters: &[(&str, &str)],
    user: &str,
    password: &[u8],
) -> Result<Session, StartError> {
    let size = parameters.iter().try_fold(9_usize, |size, (key, value)| {
        size.checked_add(key.len())?
            .checked_add(value.len())?
            .checked_add(2)
    });
    if size.is_none_or(|size| size > MAX_MESSAGE) || password.len() > MAX_MESSAGE - 5 {
        return Err(invalid("PostgreSQL startup data exceeds the 64 MiB limit").into());
    }
    transport
        .send(|out| frontend::startup_message(parameters.iter().copied(), out))
        .await?;
    match next(transport).await? {
        Message::AuthenticationOk => {}
        Message::AuthenticationCleartextPassword => {
            transport
                .send(|out| frontend::password_message(password, out))
                .await?;
            auth_ok(transport).await?;
        }
        Message::AuthenticationMd5Password(request) => {
            let password = md5_hash(user.as_bytes(), password, request.salt());
            transport
                .send(|out| frontend::password_message(password.as_bytes(), out))
                .await?;
            auth_ok(transport).await?;
        }
        Message::AuthenticationSasl(request) => {
            use fallible_iterator::FallibleIterator;
            if !request
                .mechanisms()
                .any(|mechanism| Ok(mechanism == SCRAM_SHA_256))?
            {
                return Err(invalid("unsupported PostgreSQL SASL authentication mechanism").into());
            }
            // The TLS adapter does not supply channel binding data. Offer
            // ordinary SCRAM only, including on a TLS socket.
            scram(transport, password).await?;
            auth_ok(transport).await?;
        }
        Message::AuthenticationGss
        | Message::AuthenticationGssContinue(_)
        | Message::AuthenticationKerberosV5
        | Message::AuthenticationScmCredential
        | Message::AuthenticationSspi => {
            return Err(invalid("unsupported PostgreSQL authentication method").into());
        }
        _ => return Err(invalid("expected PostgreSQL authentication request").into()),
    }

    let mut session = Session {
        parameters: BTreeMap::new(),
        pid: 0,
        status: b'I',
    };
    loop {
        match next(transport).await? {
            Message::ParameterStatus(parameter) => {
                session
                    .parameters
                    .insert(parameter.name()?.into(), parameter.value()?.into());
            }
            Message::BackendKeyData(key) => {
                session.pid = key.process_id();
            }
            Message::ReadyForQuery(ready) => {
                session.status = super::ready_status(ready.status())?;
                return Ok(session);
            }
            _ => return Err(invalid("unexpected PostgreSQL startup response").into()),
        }
    }
}

async fn auth_ok(transport: &mut Transport) -> Result<(), StartError> {
    if matches!(next(transport).await?, Message::AuthenticationOk) {
        Ok(())
    } else {
        Err(invalid("expected PostgreSQL authentication completion").into())
    }
}

async fn scram(transport: &mut Transport, password: &[u8]) -> Result<(), StartError> {
    let mut scram = ScramSha256::new(password, ChannelBinding::unsupported());
    transport
        .send(|out| frontend::sasl_initial_response(SCRAM_SHA_256, scram.message(), out))
        .await?;
    let Message::AuthenticationSaslContinue(first) = next(transport).await? else {
        return Err(invalid("expected PostgreSQL SCRAM server-first message").into());
    };
    scram.update(first.data())?;
    if scram.message().len() > MAX_MESSAGE - 4 {
        return Err(invalid("PostgreSQL SCRAM response exceeds the 64 MiB limit").into());
    }
    transport
        .send(|out| frontend::sasl_response(scram.message(), out))
        .await?;
    let Message::AuthenticationSaslFinal(last) = next(transport).await? else {
        return Err(invalid("expected PostgreSQL SCRAM server-final message").into());
    };
    scram.finish(last.data())?;
    Ok(())
}

async fn next(transport: &mut Transport) -> Result<Message, StartError> {
    loop {
        match transport.receive().await? {
            Message::NoticeResponse(_) => {}
            Message::ErrorResponse(error) => {
                return Err(StartError::Server(super::server_error(
                    error.fields(),
                    "",
                    false,
                )?));
            }
            message => return Ok(message),
        }
    }
}
