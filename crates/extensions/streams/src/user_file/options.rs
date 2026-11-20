//! `flock($stream, LOCK_EX)` asks the wrapper to acquire the requested lock.

use phera_extension_api::{ArrayKey, Context, NativeError, Value, value::to_bool};

use super::{UserFile, missing};

impl UserFile {
    pub async fn lock(
        &self,
        context: &mut dyn Context,
        function: &str,
        operation: i64,
    ) -> Result<bool, NativeError> {
        match self
            .invoke(context, "stream_lock", vec![Value::Int(operation)])
            .await?
        {
            Some(Value::Bool(value)) => Ok(value),
            None if operation == 0 => Ok(true),
            None => {
                missing(context, &self.object, function, "stream_lock", "");
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    pub async fn truncate(
        &self,
        context: &mut dyn Context,
        function: &str,
        size: u64,
    ) -> Result<bool, NativeError> {
        if !context.has_method(&self.object, "stream_truncate") {
            context.warning(format!("{function}(): Can't truncate this stream!"));
            return Ok(false);
        }
        match self
            .invoke(context, "stream_truncate", vec![Value::Int(size as i64)])
            .await?
        {
            Some(Value::Bool(value)) => Ok(value),
            Some(value) => {
                context.warning(format!(
                    "{function}(): {}::stream_truncate value must be of type bool, {} given",
                    self.object.class.name(),
                    crate::resource::given(&value)
                ));
                Ok(false)
            }
            None => Ok(false),
        }
    }
    pub async fn option(
        &self,
        context: &mut dyn Context,
        function: &str,
        option: i64,
        value: i64,
        extra: Value,
    ) -> Result<bool, NativeError> {
        match self
            .invoke(
                context,
                "stream_set_option",
                vec![Value::Int(option), Value::Int(value), extra],
            )
            .await?
        {
            Some(value) => Ok(to_bool(&value)),
            None => {
                missing(context, &self.object, function, "stream_set_option", "");
                Ok(false)
            }
        }
    }

    pub fn metadata(&self) -> Value {
        let mut metadata = phera_extension_api::PhpArray::new();
        for (key, value) in [
            ("timed_out", Value::Bool(false)),
            ("blocked", Value::Bool(true)),
            ("eof", Value::Bool(self.eof.get())),
            ("wrapper_data", Value::Object(self.object.clone())),
            ("wrapper_type", Value::from("user-space")),
            ("stream_type", Value::from("user-space")),
            ("mode", Value::from(self.mode.clone())),
            (
                "unread_bytes",
                Value::Int(self.buffer.borrow().len() as i64),
            ),
            ("seekable", Value::Bool(self.seekable.get())),
            ("uri", Value::from(self.uri.clone())),
        ] {
            metadata.insert(ArrayKey::str(key), value);
        }
        Value::from(metadata)
    }
}
