//! MULTI and WATCH retain the object's exclusive lease; pipeline stores owned command plans.
use crate::commands::Plan;
use crate::connection::{Session, exception, number};
use crate::options::Options;
use phera_extension_api::{Context, NativeResult, Object, PhpArray, Value};
use std::rc::Rc;

pub enum Effect {
    Database(i64),
    Auth(Value),
}

pub struct Batch {
    pub transaction: bool,
    pub commands: Vec<(Plan, Options)>,
    pub effects: Vec<(usize, Effect)>,
}

pub async fn control(
    name: &str,
    c: &mut dyn Context,
    object: &Rc<Object>,
    s: &Session,
    arguments: &[Value],
) -> NativeResult {
    match name {
        "multi" | "pipeline" => {
            if s.batch.borrow().is_some() {
                return Ok(Value::Bool(false));
            }
            let mode = if name == "pipeline" {
                2
            } else {
                number(arguments.first(), 1.0) as i64
            };
            if !matches!(mode, 1 | 2) {
                return Err(exception("Unknown mode sent to Redis::multi"));
            }
            s.ensure().await?;
            if mode == 1 {
                s.query(redis::cmd("MULTI")).await?;
            }
            *s.batch.borrow_mut() = Some(Batch {
                transaction: mode == 1,
                commands: Vec::new(),
                effects: Vec::new(),
            });
            Ok(Value::Object(object.clone()))
        }
        "discard" => {
            let batch = s.batch.borrow_mut().take();
            if let Some(batch) = batch {
                if batch.transaction {
                    s.query(redis::cmd("DISCARD")).await?;
                }
                Ok(Value::Bool(true))
            } else {
                Ok(Value::Bool(false))
            }
        }
        "exec" => {
            let batch = s.batch.borrow_mut().take();
            let Some(batch) = batch else {
                return Ok(Value::Bool(false));
            };
            let mut result = PhpArray::new();
            let replies = if batch.transaction {
                let reply = s.query(redis::cmd("EXEC")).await?;
                if matches!(reply, redis::Value::Nil) {
                    return Ok(Value::Bool(false));
                }
                crate::reply::items(reply)
            } else {
                let commands = batch
                    .commands
                    .iter()
                    .map(|(p, _)| p.command.clone())
                    .collect();
                let timeout = s.options.borrow().read_timeout;
                s.connected()?
                    .pipeline(commands, timeout)
                    .await
                    .map_err(|e| exception(e.to_string()))?
            };
            let mut effects = batch.effects.into_iter().peekable();
            for (index, ((plan, options), reply)) in
                batch.commands.into_iter().zip(replies).enumerate()
            {
                if let redis::Value::ServerError(error) = &reply {
                    *s.error.borrow_mut() = Some(format!(
                        "{} {}",
                        error.code(),
                        error.details().unwrap_or_default()
                    ));
                }
                while effects.peek().is_some_and(|(i, _)| *i == index) {
                    let (_, effect) = effects.next().expect("peeked effect");
                    if matches!(reply, redis::Value::Okay) {
                        match effect {
                            Effect::Database(db) => s.database.set(db),
                            Effect::Auth(auth) => *s.auth.borrow_mut() = auth,
                        }
                    }
                }
                result.push(crate::reply::convert(c, &options, &plan.shape, reply).await?)?;
            }
            Ok(Value::from(result))
        }
        _ => unreachable!(),
    }
}

pub async fn queue(
    s: &Session,
    plan: Plan,
    options: Options,
) -> Result<(), phera_extension_api::NativeError> {
    let transaction = s.batch.borrow().as_ref().is_some_and(|b| b.transaction);
    if transaction {
        s.query(plan.command.clone()).await?;
    }
    if let Some(batch) = s.batch.borrow_mut().as_mut() {
        batch.commands.push((plan, options));
    }
    Ok(())
}

pub async fn queue_state(
    name: &str,
    s: &Session,
    value: &Value,
) -> Result<(), phera_extension_api::NativeError> {
    let mut command = redis::cmd(&name.to_uppercase());
    let effect = if name == "select" {
        let db = number(Some(value), 0.0) as i64;
        command.arg(db);
        Effect::Database(db)
    } else {
        match value {
            Value::Array(a) => {
                for (_, v) in a.iter() {
                    command.arg(crate::connection::string(v));
                }
            }
            _ => {
                command.arg(crate::connection::string(value));
            }
        }
        Effect::Auth(value.clone())
    };
    let options = s.options.borrow().clone();
    queue(
        s,
        Plan {
            command,
            shape: crate::reply::Shape::Raw,
        },
        options,
    )
    .await?;
    if let Some(batch) = s.batch.borrow_mut().as_mut() {
        batch.effects.push((batch.commands.len() - 1, effect));
    }
    Ok(())
}
