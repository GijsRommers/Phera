//! Command plans contain owned wire bytes and the PHP reply shape, never PHP values.
mod collections;
mod geo;
mod server;
mod sort;
mod sorted;
mod streams;
mod strings;

use crate::connection::{exception, string};
use crate::options::Options;
use crate::reply::Shape;
use phera_extension_api::{Context, NativeError, Value};

pub struct Plan {
    pub command: redis::Cmd,
    pub shape: Shape,
}

pub struct Builder<'a> {
    pub command: redis::Cmd,
    pub shape: Shape,
    pub options: &'a Options,
}

impl<'a> Builder<'a> {
    pub fn new(name: &str, options: &'a Options) -> Self {
        Self {
            command: redis::cmd(&name.to_ascii_uppercase()),
            shape: Shape::Raw,
            options,
        }
    }
    pub fn key(&mut self, value: &Value) {
        let mut bytes = self.options.prefix.clone();
        match value {
            Value::String(s) => bytes.extend_from_slice(s.as_bytes()),
            _ => bytes.extend_from_slice(string(value).as_bytes()),
        }
        self.command.arg(bytes);
    }
    pub fn arg(&mut self, value: &Value) {
        match value {
            Value::String(s) => {
                self.command.arg(s.as_bytes());
            }
            Value::Bool(v) => {
                self.command.arg(i32::from(*v));
            }
            _ => {
                self.command.arg(string(value));
            }
        }
    }
    pub fn token(&mut self, value: impl redis::ToRedisArgs) {
        self.command.arg(value);
    }
    pub async fn value(
        &mut self,
        context: &mut dyn Context,
        value: &Value,
    ) -> Result<(), NativeError> {
        self.command
            .arg(crate::options::serialize(context, self.options, value).await?);
        Ok(())
    }
    pub fn keys(&mut self, args: &[Value]) {
        for value in args {
            if let Value::Array(values) = value {
                for (_, value) in values.iter() {
                    self.key(value);
                }
            } else {
                self.key(value);
            }
        }
    }
    pub fn args(&mut self, args: &[Value]) {
        for value in args {
            if let Value::Array(values) = value {
                for (_, value) in values.iter() {
                    self.arg(value);
                }
            } else {
                self.arg(value);
            }
        }
    }
}

pub async fn build(
    name: &str,
    context: &mut dyn Context,
    args: &[Value],
    options: &Options,
) -> Result<Plan, NativeError> {
    let mut builder = Builder::new(name, options);
    let found = strings::build(name, context, args, &mut builder).await?
        || collections::build(name, context, args, &mut builder).await?
        || sorted::build(name, context, args, &mut builder).await?
        || streams::build(name, context, args, &mut builder).await?
        || geo::build(name, args, &mut builder)?
        || server::build(name, args, &mut builder)?
        || sort::build(name, args, &mut builder);
    if !found {
        return Err(exception(format!("Redis::{name} is not implemented yet")));
    }
    Ok(Plan {
        command: builder.command,
        shape: builder.shape,
    })
}

pub fn array(value: Option<&Value>) -> Option<&phera_extension_api::PhpArray> {
    match value {
        Some(Value::Array(a)) => Some(a),
        _ => None,
    }
}

pub fn option<'a>(value: Option<&'a Value>, name: &str) -> Option<&'a Value> {
    array(value)?.iter().find_map(|(key, value)| match key {
        phera_extension_api::ArrayKey::String(s)
            if s.to_string_lossy().eq_ignore_ascii_case(name) =>
        {
            Some(value)
        }
        _ => None,
    })
}

pub fn flags(builder: &mut Builder<'_>, value: Option<&Value>) {
    if let Some(options) = array(value) {
        for (key, value) in options.iter() {
            match key {
                phera_extension_api::ArrayKey::Int(_) => {
                    builder.token(string(value).to_uppercase())
                }
                phera_extension_api::ArrayKey::String(key) => {
                    builder.token(key.to_string_lossy().to_uppercase());
                    if let Value::Array(items) = value {
                        for (_, item) in items.iter() {
                            builder.arg(item);
                        }
                    } else {
                        builder.arg(value);
                    }
                }
            }
        }
    }
}
