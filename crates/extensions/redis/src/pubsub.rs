//! Subscription sockets are dedicated and never returned to a pool.
//! `$r->subscribe(["news"], function ($r, $channel, $message) { $r->unsubscribe(["news"]); });`
use crate::connection::{Session, exception, string};
use phera_extension_api::{Context, NativeResult, Object, PhpArray, Value};
use redis::{PushKind, Value as Reply};
use std::collections::HashSet;
use std::rc::Rc;

pub struct Subscription {
    connection: redis::aio::MultiplexedConnection,
    channels: HashSet<Vec<u8>>,
}

pub async fn subscribe(
    name: &str,
    c: &mut dyn Context,
    object: &Rc<Object>,
    s: &Session,
    a: &[Value],
) -> NativeResult {
    let endpoint = s
        .endpoint
        .borrow()
        .clone()
        .ok_or_else(|| exception("Redis server went away"))?;
    let client = endpoint.client().map_err(|e| exception(e.to_string()))?;
    let mut info = client.get_connection_info().clone();
    let mut settings = info
        .redis_settings()
        .clone()
        .set_protocol(redis::ProtocolVersion::RESP3);
    let auth = s.auth.borrow().clone();
    match auth {
        Value::String(password) => settings = settings.set_password(password.to_string_lossy()),
        Value::Array(values) => {
            let mut values = values.iter().map(|(_, v)| string(v));
            if let Some(username) = values.next() {
                settings = settings.set_username(username);
            }
            if let Some(password) = values.next() {
                settings = settings.set_password(password);
            }
        }
        _ => (),
    }
    info = info.set_redis_settings(settings);
    let prefix = s.options.borrow().prefix.clone();
    let channels: Vec<Vec<u8>> = crate::commands::array(a.first())
        .map(|values| {
            values
                .iter()
                .map(|(_, v)| match v {
                    Value::String(s) => s.as_bytes().to_vec(),
                    _ => string(v).into_bytes(),
                })
                .map(|channel| [prefix.as_slice(), channel.as_slice()].concat())
                .collect()
        })
        .unwrap_or_default();
    if channels.is_empty() {
        return Ok(Value::Bool(false));
    }
    let (sender, mut messages) = tokio::sync::mpsc::unbounded_channel();
    let handle = crate::pool::runtime()
        .await
        .map_err(|e| exception(e.to_string()))?;
    let mut connection = handle
        .spawn(async move {
            redis::Client::open(info)?
                .get_multiplexed_async_connection_with_config(
                    &redis::AsyncConnectionConfig::new().set_push_sender(sender),
                )
                .await
        })
        .await
        .map_err(|e| exception(e.to_string()))?
        .map_err(|e| exception(e.to_string()))?;
    let registered = channels.clone();
    let method = name.to_uppercase();
    connection = handle
        .spawn(async move {
            redis::cmd(&method)
                .arg(channels)
                .query_async::<Reply>(&mut connection)
                .await?;
            Ok::<_, redis::RedisError>(connection)
        })
        .await
        .map_err(|e| exception(e.to_string()))?
        .map_err(|e| exception(e.to_string()))?;
    *s.subscriptions.borrow_mut() = Some(Subscription {
        connection,
        channels: registered.into_iter().collect(),
    });
    let result = async {
        while s
            .subscriptions
            .borrow()
            .as_ref()
            .is_some_and(|sub| !sub.channels.is_empty())
        {
            let Some(message) = messages.recv().await else {
                break;
            };
            if !matches!(
                message.kind,
                PushKind::Message | PushKind::PMessage | PushKind::SMessage
            ) {
                continue;
            }
            let mut arguments = vec![Value::Object(object.clone())];
            for item in message.data {
                arguments.push(Value::from(crate::reply::bytes(item)));
            }
            c.call(&a[1], arguments).await?;
        }
        Ok(Value::Bool(true))
    }
    .await;
    s.subscriptions.borrow_mut().take();
    result
}

pub async fn unsubscribe(name: &str, s: &Session, a: &[Value]) -> NativeResult {
    let (mut connection, channels) = {
        let mut state = s.subscriptions.borrow_mut();
        let Some(sub) = state.as_mut() else {
            return Ok(Value::Bool(false));
        };
        let supplied: Vec<Vec<u8>> = crate::commands::array(a.first())
            .map(|values| {
                values
                    .iter()
                    .map(|(_, v)| {
                        let mut channel = s.options.borrow().prefix.clone();
                        if let Value::String(s) = v {
                            channel.extend_from_slice(s.as_bytes());
                        } else {
                            channel.extend_from_slice(string(v).as_bytes());
                        }
                        channel
                    })
                    .collect()
            })
            .unwrap_or_default();
        if supplied.is_empty() {
            return Ok(Value::Bool(false));
        }
        let channels = supplied;
        for channel in &channels {
            sub.channels.remove(channel);
        }
        (sub.connection.clone(), channels)
    };
    let method = name.to_uppercase();
    let sent = channels.clone();
    crate::pool::runtime()
        .await
        .map_err(|e| exception(e.to_string()))?
        .spawn(async move {
            redis::cmd(&method)
                .arg(sent)
                .query_async::<Reply>(&mut connection)
                .await
        })
        .await
        .map_err(|e| exception(e.to_string()))?
        .map_err(|e| exception(e.to_string()))?;
    let mut result = PhpArray::new();
    for channel in channels {
        result.insert(
            phera_extension_api::ArrayKey::from_value(&Value::from(channel))?,
            Value::Bool(true),
        );
    }
    Ok(Value::from(result))
}
