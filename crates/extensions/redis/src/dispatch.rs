//! All network methods are async native calls; local object state never leaves the worker.
use crate::connection::{exception, number, session};
use phera_extension_api::{Context, LocalFuture, NativeResult, Object, Value};
use std::rc::Rc;

pub fn dispatch<'a>(
    name: &'static str,
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    if let Some(result) = local(name, context, this.as_ref(), &arguments) {
        return Box::pin(std::future::ready(result));
    }
    Box::pin(async move {
        let object = this.ok_or_else(|| exception("Redis object is missing"))?;
        let session = session(&object);
        let a = &arguments;
        match name {
            "connect" | "open" | "pconnect" | "popen" => {
                return crate::connection::connect(name, &session, a).await;
            }
            "__destruct" => {
                let transaction = session
                    .batch
                    .borrow()
                    .as_ref()
                    .is_some_and(|b| b.transaction);
                if transaction {
                    // phpredis discards MULTI but keeps its local mode until exec/discard.
                    session.query(redis::cmd("DISCARD")).await?;
                }
                return Ok(Value::Null);
            }
            "gethost" | "getport" | "getpersistentid" | "gettimeout" => {
                session.ensure().await?;
                let endpoint = session.endpoint.borrow();
                return Ok(match (name, endpoint.as_ref()) {
                    ("gethost", Some(e)) => Value::from(e.host.clone()),
                    ("getport", Some(e)) => Value::Int(i64::from(e.port)),
                    ("gettimeout", Some(e)) => Value::Float(e.timeout_micros as f64 / 1_000_000.0),
                    ("getpersistentid", Some(e)) => e
                        .persistent_id
                        .as_ref()
                        .map_or(Value::Null, |id| Value::from(id.clone())),
                    _ => Value::Bool(false),
                });
            }
            "servername" | "serverversion" => {
                let mut command = redis::cmd("INFO");
                command.arg("server");
                let data = crate::reply::bytes(session.query(command).await?);
                let text = String::from_utf8_lossy(&data);
                return Ok(if name == "servername" {
                    Value::from("redis")
                } else {
                    text.lines()
                        .find_map(|line| line.strip_prefix("redis_version:"))
                        .map_or(Value::Bool(false), Value::from)
                });
            }
            "getdbnum" => {
                session.ensure().await?;
                return Ok(Value::Int(session.database.get()));
            }
            "_serialize" | "_pack" => {
                let options = session.options.borrow().clone();
                return Ok(Value::from(
                    crate::options::serialize(context, &options, &a[0]).await?,
                ));
            }
            "_unserialize" | "_unpack" => {
                let options = session.options.borrow().clone();
                if let Some(value) = a[0].as_str_bytes() {
                    return crate::options::deserialize(context, &options, value.to_vec()).await;
                }
            }
            "auth" | "select" => {
                let value = if name == "auth" {
                    let Some(value) = crate::auth::credentials(context, &a[0], "auth")? else {
                        return Ok(Value::Bool(false));
                    };
                    value
                } else {
                    a[0].clone()
                };
                if session.batch.borrow().is_some() {
                    if !crate::transaction::queue_state(name, &session, &value).await? {
                        return Ok(Value::Bool(false));
                    }
                    if name == "auth" {
                        *session.auth.borrow_mut() = value;
                    }
                    return Ok(Value::Object(object));
                }
                return if name == "auth" {
                    crate::connection::auth(&session, &value).await
                } else {
                    crate::connection::select(&session, &value).await
                };
            }
            "reset" => {
                // RESET goes out at once, also during MULTI, which it ends,
                // and leaves the connection in database 0. phpredis raises
                // a fatal error for it in pipeline mode; there it is queued.
                let pipelining = session
                    .batch
                    .borrow()
                    .as_ref()
                    .is_some_and(|batch| !batch.transaction);
                if !pipelining {
                    let reply = session.query(redis::cmd("RESET")).await?;
                    session.batch.borrow_mut().take();
                    session.database.set(0);
                    return Ok(Value::Bool(!matches!(reply, redis::Value::ServerError(_))));
                }
            }
            "multi" | "pipeline" | "exec" | "discard" => {
                return crate::transaction::control(name, context, &object, &session, a).await;
            }
            "scan" | "hscan" | "sscan" | "zscan" => {
                return crate::scan::scan(name, context, &session, a).await;
            }
            "subscribe" | "psubscribe" | "ssubscribe" => {
                return crate::pubsub::subscribe(name, context, &object, &session, a).await;
            }
            "unsubscribe" | "punsubscribe" | "sunsubscribe" => {
                return crate::pubsub::unsubscribe(name, &session, a).await;
            }
            _ => (),
        }
        if name == "sintercard" && a.get(1).is_some_and(|v| number(Some(v), 0.0) < 0.0) {
            context.warning("Redis::sintercard(): LIMIT cannot be negative".into());
            return Ok(Value::Bool(false));
        }
        let options = session.options.borrow().clone();
        if name == "rawcommand"
            && a.first().is_some_and(|v| {
                matches!(
                    crate::connection::string(v).to_uppercase().as_str(),
                    "SUBSCRIBE" | "PSUBSCRIBE" | "SSUBSCRIBE" | "MONITOR"
                )
            })
        {
            session
                .connected()?
                .retire()
                .await
                .map_err(|e| exception(e.to_string()))?;
        }
        let plan = crate::commands::build(name, context, a, &options).await?;
        if session.batch.borrow().is_some() {
            return Ok(
                if crate::transaction::queue(&session, plan, options).await? {
                    Value::Object(object)
                } else {
                    Value::Bool(false)
                },
            );
        }
        let previous_error = session.error.borrow().clone();
        let reply = session.query(plan.command).await?;
        if name == "lpos" {
            *session.error.borrow_mut() = previous_error;
        }
        crate::reply::convert(context, &options, &plan.shape, reply).await
    })
}

/// The methods that only read or change the object's own state. They answer
/// at once, without allocating the much larger future of a command; `None`
/// for every other method.
fn local(
    name: &str,
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    a: &[Value],
) -> Option<NativeResult> {
    let Some(object) = this else {
        return Some(Err(exception("Redis object is missing")));
    };
    let session = session(object);
    let result = match name {
        "__construct" => crate::constructor::construct(context, &session, a),
        "close" => {
            let connected = session.lease.borrow().is_some();
            session.lease.borrow_mut().take();
            session.batch.borrow_mut().take();
            Ok(Value::Bool(connected))
        }
        "isconnected" => Ok(Value::Bool(session.lease.borrow().is_some())),
        "getauth" => Ok(session.auth.borrow().clone()),
        "getreadtimeout" => Ok(Value::Float(session.options.borrow().read_timeout)),
        "getlasterror" => Ok(session
            .error
            .borrow()
            .as_ref()
            .map_or(Value::Null, |error| Value::from(error.clone()))),
        "clearlasterror" => {
            session.error.borrow_mut().take();
            Ok(Value::Bool(true))
        }
        "setoption" => crate::options::set(context, &session, number(a.first(), 0.0) as i64, &a[1]),
        "getoption" => crate::options::get(&session, number(a.first(), 0.0) as i64),
        "getmode" => Ok(Value::Int(
            session
                .batch
                .borrow()
                .as_ref()
                .map_or(0, |batch| if batch.transaction { 1 } else { 2 }),
        )),
        "_prefix" => {
            let mut bytes = session.options.borrow().prefix.clone();
            if let Some(value) = a[0].as_str_bytes() {
                bytes.extend_from_slice(value);
            }
            Ok(Value::from(bytes))
        }
        "_compress" | "_uncompress" => Ok(a[0].clone()),
        _ => return None,
    };
    Some(result)
}
