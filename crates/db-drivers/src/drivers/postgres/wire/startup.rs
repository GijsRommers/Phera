//! Startup negotiation on a socket after applying the DSN's TLS policy.

use std::collections::BTreeMap;
use std::io;

use fallible_iterator::FallibleIterator;
use postgres_protocol::authentication::md5_hash;
use postgres_protocol::authentication::sasl::{
    ChannelBinding, SCRAM_SHA_256, SCRAM_SHA_256_PLUS, ScramSha256,
};
use postgres_protocol::message::{
    backend::{AuthenticationSaslBody, Message},
    frontend,
};

use super::{MAX_MESSAGE, Transport, invalid};
use crate::drivers::connection_error;
use crate::dsn::Dsn;
use crate::errors::DbError;

mod binding;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(in super::super) enum BindingMode {
    Disable,
    #[default]
    Prefer,
    Require,
}

#[derive(Clone, Copy)]
enum Method {
    None,
    Password,
    Md5,
    Scram,
    Gss,
    Sspi,
    Oauth,
}

impl Method {
    fn mask(self) -> u8 {
        1 << self as u8
    }
}

pub(in super::super) struct AuthPolicy<'a> {
    pub binding: BindingMode,
    required: Option<&'a str>,
    allowed: u8,
}

impl Default for AuthPolicy<'_> {
    fn default() -> Self {
        Self {
            binding: BindingMode::Prefer,
            required: None,
            allowed: u8::MAX,
        }
    }
}

impl<'a> AuthPolicy<'a> {
    pub fn parse(dsn: &'a Dsn) -> Result<Self, DbError> {
        let binding = match dsn.get("channel_binding", "prefer") {
            "disable" => BindingMode::Disable,
            "prefer" => BindingMode::Prefer,
            "require" => BindingMode::Require,
            _ => return Err(connection_error("invalid channel_binding value")),
        };
        let required = dsn.fields.get("require_auth").map(String::as_str);
        let allowed = required.map(parse_methods).transpose()?.unwrap_or(u8::MAX);
        Ok(Self {
            binding,
            required,
            allowed,
        })
    }

    fn check(&self, request: &Message) -> io::Result<()> {
        let (method, challenge) = match request {
            Message::AuthenticationOk => (Method::None, "server did not complete authentication"),
            Message::AuthenticationCleartextPassword => {
                (Method::Password, "server requested a cleartext password")
            }
            Message::AuthenticationMd5Password(_) => {
                (Method::Md5, "server requested a hashed password")
            }
            Message::AuthenticationSasl(_) => {
                (Method::Scram, "server requested SASL authentication")
            }
            _ => return Ok(()),
        };
        if self.allowed & method.mask() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "authentication method requirement \"{}\" failed: {challenge}",
                    self.required.unwrap_or_default()
                ),
            ));
        }
        if self.binding == BindingMode::Require && !matches!(method, Method::Scram) {
            return Err(invalid(
                "channel binding required, but server did not offer an authentication method that supports channel binding",
            ));
        }
        Ok(())
    }
}

fn parse_methods(value: &str) -> Result<u8, DbError> {
    if value.is_empty() {
        return Ok(Method::None.mask());
    }
    let negated = value.starts_with('!');
    let mut methods = 0;
    for item in value.split(',') {
        if negated != item.starts_with('!') {
            return Err(connection_error(
                "negative and non-negative require_auth methods cannot be mixed",
            ));
        }
        let method = match item.strip_prefix('!').unwrap_or(item) {
            "none" => Method::None,
            "password" => Method::Password,
            "md5" => Method::Md5,
            "scram-sha-256" => Method::Scram,
            "gss" => Method::Gss,
            "sspi" => Method::Sspi,
            "oauth" => Method::Oauth,
            _ => return Err(connection_error("invalid require_auth value")),
        }
        .mask();
        if methods & method != 0 {
            return Err(connection_error(
                "require_auth method specified more than once",
            ));
        }
        methods |= method;
    }
    Ok(if negated { !methods } else { methods })
}

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
    pub parameters: Parameters,
    pub pid: i32,
    pub status: u8,
}

#[derive(Default)]
pub(in super::super) struct Parameters {
    entries: BTreeMap<String, String>,
    bytes: usize,
}

impl Parameters {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(String::as_str)
    }

    pub fn set(&mut self, name: &str, value: &str) -> io::Result<()> {
        const MAX_COUNT: usize = 128;
        const MAX_BYTES: usize = 128 * 1024;
        let prior = self.get(name);
        let byte_count = self.bytes
            + value.len()
            + match prior {
                Some(_) => 0,
                None => name.len(),
            }
            - prior.map_or(0, str::len);
        if byte_count > MAX_BYTES || (prior.is_none() && self.entries.len() >= MAX_COUNT) {
            return Err(invalid("PostgreSQL session parameters exceed the limit"));
        }
        if let Some(prior) = self.entries.get_mut(name) {
            *prior = value.to_owned();
        } else {
            self.entries.insert(name.into(), value.into());
        }
        self.bytes = byte_count;
        Ok(())
    }
}

pub(in super::super) async fn start(
    transport: &mut Transport,
    parameters: &[(&str, &str)],
    user: &str,
    password: &[u8],
    policy: &AuthPolicy<'_>,
    certificate: Option<&[u8]>,
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
    let request = next(transport).await?;
    if policy.binding == BindingMode::Require && certificate.is_none() {
        return Err(invalid("channel binding required, but SSL not in use").into());
    }
    policy.check(&request)?;
    match request {
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
            let (mechanism, binding) = scram_mechanism(&request, policy.binding, certificate)?;
            scram(transport, password, mechanism, binding).await?;
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
        parameters: Parameters::default(),
        pid: 0,
        status: b'I',
    };
    loop {
        match next(transport).await? {
            Message::ParameterStatus(parameter) => {
                session
                    .parameters
                    .set(parameter.name()?, parameter.value()?)?;
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

fn scram_mechanism(
    request: &AuthenticationSaslBody,
    mode: BindingMode,
    certificate: Option<&[u8]>,
) -> io::Result<(&'static str, ChannelBinding)> {
    let certificate = certificate.filter(|_| mode != BindingMode::Disable);
    let mut plain = false;
    let mut plus = false;
    let mut mechanisms = request.mechanisms();
    while let Some(mechanism) = mechanisms.next()? {
        plain |= mechanism == SCRAM_SHA_256;
        plus |= mechanism == SCRAM_SHA_256_PLUS;
    }
    if let Some(certificate) = certificate.filter(|_| plus) {
        let digest = binding::certificate_hash(certificate).ok_or_else(|| {
            invalid("could not determine the TLS certificate channel binding hash")
        })?;
        return Ok((
            SCRAM_SHA_256_PLUS,
            ChannelBinding::tls_server_end_point(digest),
        ));
    }
    if mode == BindingMode::Require {
        return Err(invalid(
            "channel binding required, but server did not offer it",
        ));
    }
    if !plain {
        return Err(invalid(
            "unsupported PostgreSQL SASL authentication mechanism",
        ));
    }
    let binding = if certificate.is_some() {
        ChannelBinding::unrequested()
    } else {
        ChannelBinding::unsupported()
    };
    Ok((SCRAM_SHA_256, binding))
}

async fn scram(
    transport: &mut Transport,
    password: &[u8],
    mechanism: &str,
    binding: ChannelBinding,
) -> Result<(), StartError> {
    let mut scram = ScramSha256::new(password, binding);
    transport
        .send(|out| frontend::sasl_initial_response(mechanism, scram.message(), out))
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
