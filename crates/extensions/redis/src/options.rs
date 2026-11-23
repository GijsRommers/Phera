//! `$r->setOption(Redis::OPT_PREFIX, "app:")` affects keys, never payload bytes.
use crate::connection::{Session, exception, number};
use phera_extension_api::{Context, NativeResult, Value};

#[derive(Clone, Copy, Default)]
pub enum Serializer {
    #[default]
    None,
    Json,
}

#[derive(Clone, Default)]
pub struct Options {
    pub prefix: Vec<u8>,
    pub serializer: Serializer,
    pub read_timeout: f64,
    pub scan: i64,
    pub null_multibulk: bool,
    pub reply_literal: bool,
}

pub fn set(_: &mut dyn Context, session: &Session, option: i64, value: &Value) -> NativeResult {
    let mut options = session.options.borrow_mut();
    match option {
        1 => {
            options.serializer = match number(Some(value), -1.0) as i64 {
                0 => Serializer::None,
                4 => Serializer::Json,
                _ => return Err(exception("Invalid serializer")),
            }
        }
        2 => {
            options.prefix = match value {
                Value::String(s) => s.as_bytes().to_vec(),
                _ => crate::connection::string(value).into_bytes(),
            }
        }
        3 => {
            let timeout = number(Some(value), 0.0);
            if !timeout.is_finite() || timeout < 0.0 {
                return Ok(Value::Bool(false));
            }
            options.read_timeout = timeout;
        }
        4 => match number(Some(value), 0.0) as i64 {
            0 => options.scan &= !1,
            1 => options.scan |= 1,
            2 => options.scan |= 2,
            3 => options.scan &= !2,
            _ => return Ok(Value::Bool(false)),
        },
        7 if number(Some(value), 0.0) == 0.0 => (),
        10 => options.null_multibulk = phera_extension_api::value::to_bool(value),
        8 => options.reply_literal = phera_extension_api::value::to_bool(value),
        _ => return Ok(Value::Bool(false)),
    }
    Ok(Value::Bool(true))
}

pub fn get(session: &Session, option: i64) -> NativeResult {
    let options = session.options.borrow();
    Ok(match option {
        1 => Value::Int(match options.serializer {
            Serializer::None => 0,
            Serializer::Json => 4,
        }),
        2 if options.prefix.is_empty() => Value::Null,
        2 => Value::from(options.prefix.clone()),
        3 => Value::Float(options.read_timeout),
        4 => Value::Int(options.scan),
        7 => Value::Int(0),
        8 => Value::Int(i64::from(options.reply_literal)),
        10 => Value::Int(i64::from(options.null_multibulk)),
        _ => Value::Bool(false),
    })
}

pub async fn serialize(
    context: &mut dyn Context,
    options: &Options,
    value: &Value,
) -> Result<Vec<u8>, phera_extension_api::NativeError> {
    match options.serializer {
        Serializer::None if matches!(value, Value::Array(_)) => Ok(b"Array".to_vec()),
        Serializer::None => Ok(phera_extension_api::value::to_php_string(
            value,
            context.diagnostics(),
        )?
        .as_bytes()
        .to_vec()),
        Serializer::Json => {
            let value = context
                .call(&Value::from("json_encode"), vec![value.clone()])
                .await?;
            Ok(match value {
                Value::String(s) => s.as_bytes().to_vec(),
                _ => Vec::new(),
            })
        }
    }
}

pub async fn deserialize(
    context: &mut dyn Context,
    options: &Options,
    bytes: Vec<u8>,
) -> NativeResult {
    match options.serializer {
        Serializer::None => Ok(Value::from(bytes)),
        Serializer::Json => {
            context
                .call(
                    &Value::from("json_decode"),
                    vec![Value::from(bytes), Value::Bool(true)],
                )
                .await
        }
    }
}
