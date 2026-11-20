//! `fread($stream, 2)` drains a wrapper's chunk buffer before calling PHP again.

use std::io;

use phera_extension_api::{
    ArrayKey, Context, NativeError, Value,
    value::{to_bool, to_int},
};

use super::{UserFile, missing};

impl UserFile {
    pub async fn at_end(
        &self,
        context: &mut dyn Context,
        function: &str,
    ) -> Result<bool, NativeError> {
        if !self.buffer.borrow().is_empty() {
            return Ok(false);
        }
        if self.eof.get() {
            return Ok(true);
        }
        self.check_eof(context, function, true).await?;
        Ok(self.eof.get())
    }

    async fn check_eof(
        &self,
        context: &mut dyn Context,
        function: &str,
        strict: bool,
    ) -> Result<bool, NativeError> {
        let result = self.invoke(context, "stream_eof", Vec::new()).await?;
        match result {
            None => {
                missing(
                    context,
                    &self.object,
                    function,
                    "stream_eof",
                    " Assuming EOF",
                );
                self.eof.set(true);
                Ok(false)
            }
            Some(Value::Bool(true)) => {
                self.eof.set(true);
                Ok(true)
            }
            Some(Value::Bool(false)) => Ok(true),
            Some(value) if strict => {
                context.warning(format!(
                    "{function}(): {}::stream_eof value must be of type bool, {} given",
                    self.object.class.name(),
                    crate::resource::given(&value)
                ));
                self.eof.set(true);
                Ok(true)
            }
            Some(value) => {
                if to_bool(&value) {
                    self.eof.set(true);
                }
                Ok(true)
            }
        }
    }

    pub async fn read(
        &self,
        context: &mut dyn Context,
        function: &str,
        length: usize,
        line: bool,
    ) -> Result<io::Result<Vec<u8>>, NativeError> {
        let mut bytes = Vec::new();
        let mut refilled = false;
        while bytes.len() < length {
            let remaining = length - bytes.len();
            let (amount, end_of_line) = {
                let buffer = self.buffer.borrow();
                let amount = remaining.min(buffer.len());
                let newline = line
                    .then(|| buffer.iter().take(amount).position(|&b| b == b'\n'))
                    .flatten();
                (newline.map_or(amount, |index| index + 1), newline.is_some())
            };
            if amount != 0 {
                bytes.extend(self.buffer.borrow_mut().drain(..amount));
                self.position
                    .set(self.position.get().saturating_add(amount as i64));
            }
            if end_of_line || bytes.len() == length || (refilled && !line) {
                break;
            }
            let chunk = self.chunk.get();
            let requested = if chunk == 1 && !line {
                remaining
            } else {
                chunk
            };
            let read = self
                .invoke(context, "stream_read", vec![Value::Int(requested as i64)])
                .await?;
            let data = match read {
                None => {
                    missing(context, &self.object, function, "stream_read", "");
                    None
                }
                Some(Value::Bool(false)) => None,
                Some(value) => Some(context.stringify(&value).await?),
            };
            let Some(data) = data else {
                return Ok(if bytes.is_empty() {
                    Err(io::ErrorKind::Other.into())
                } else {
                    Ok(bytes)
                });
            };
            let data = data.as_bytes();
            if data.len() > requested {
                context.warning(format!(
                    "{function}(): {}::stream_read - read {} bytes more data than requested ({} read, {requested} max) - excess data will be lost",
                    self.object.class.name(), data.len() - requested, data.len()
                ));
            }
            if !self.check_eof(context, function, false).await? {
                return Ok(Err(io::ErrorKind::Other.into()));
            }
            if data.is_empty() {
                break;
            }
            self.buffer
                .borrow_mut()
                .extend(&data[..data.len().min(requested)]);
            refilled = true;
        }
        Ok(Ok(bytes))
    }

    pub async fn collect(
        &self,
        context: &mut dyn Context,
        function: &str,
        length: Option<u64>,
    ) -> Result<Vec<u8>, NativeError> {
        let limited = length.is_some_and(|length| length < 4 * crate::io_policy::CHUNK as u64);
        if !limited && function != "readfile" {
            self.stat(context, function).await?;
        }
        let mut result = Vec::new();
        let mut remaining = length.unwrap_or(u64::MAX);
        while remaining > 0 {
            if limited && self.at_end(context, function).await? {
                break;
            }
            let chunk = remaining.min(crate::io_policy::CHUNK as u64) as usize;
            let Ok(bytes) = self.read(context, function, chunk, false).await? else {
                break;
            };
            if bytes.is_empty() {
                break;
            }
            remaining -= bytes.len() as u64;
            result
                .try_reserve(bytes.len())
                .map_err(|_| NativeError::error("Unable to allocate stream contents"))?;
            result.extend(bytes);
        }
        Ok(result)
    }

    pub async fn write(
        &self,
        context: &mut dyn Context,
        function: &str,
        data: &[u8],
    ) -> Result<usize, NativeError> {
        if !data.is_empty() && !self.buffer.borrow().is_empty() {
            self.seek(context, function, self.position.get(), 0).await?;
        }
        let mut written = 0;
        while written < data.len() {
            let slice = &data[written..];
            let value = self
                .invoke(context, "stream_write", vec![Value::from(slice.to_vec())])
                .await?;
            let count = match value {
                None => {
                    missing(context, &self.object, function, "stream_write", "");
                    break;
                }
                Some(Value::Bool(false)) => break,
                Some(value) => to_int(&value, context.diagnostics())?,
            };
            if count <= 0 {
                break;
            }
            let count = usize::try_from(count).unwrap_or(usize::MAX);
            if count > slice.len() {
                context.warning(format!(
                    "{function}(): {}::stream_write wrote {} bytes more data than requested ({count} written, {} max)",
                    self.object.class.name(), count - slice.len(), slice.len()
                ));
            }
            let count = count.min(slice.len());
            self.writing.set(true);
            self.position
                .set(self.position.get().saturating_add(count as i64));
            written += count;
        }
        Ok(written)
    }

    pub async fn flush(&self, context: &mut dyn Context) -> Result<bool, NativeError> {
        self.writing.set(false);
        Ok(self
            .invoke(context, "stream_flush", Vec::new())
            .await?
            .is_some_and(|value| to_bool(&value)))
    }

    pub async fn close(&self, context: &mut dyn Context) -> Result<(), NativeError> {
        if self.writing.get() {
            self.flush(context).await?;
        }
        self.invoke(context, "stream_close", Vec::new()).await?;
        Ok(())
    }

    pub async fn seek(
        &self,
        context: &mut dyn Context,
        function: &str,
        mut offset: i64,
        mut whence: i64,
    ) -> Result<bool, NativeError> {
        let current = self.position.get();
        let advance = match whence {
            0 => offset.checked_sub(current),
            1 => Some(offset),
            _ => None,
        };
        if let Some(advance) = advance.and_then(|advance| usize::try_from(advance).ok())
            && advance > 0
            && advance <= self.buffer.borrow().len()
        {
            self.buffer.borrow_mut().drain(..advance);
            self.position.set(current.saturating_add(advance as i64));
            self.eof.set(false);
            return Ok(true);
        }
        if self.seekable.get() {
            if whence == 1 {
                offset = offset.saturating_add(current);
                whence = 0;
            } else if whence == 0 && offset < 0 {
                return Ok(false);
            }
            let value = self
                .invoke(
                    context,
                    "stream_seek",
                    vec![Value::Int(offset), Value::Int(whence)],
                )
                .await?;
            if let Some(value) = value {
                self.buffer.borrow_mut().clear();
                if to_bool(&value) {
                    match self.invoke(context, "stream_tell", Vec::new()).await? {
                        Some(Value::Int(position)) => {
                            self.position.set(position);
                            self.eof.set(false);
                            return Ok(true);
                        }
                        None => missing(context, &self.object, function, "stream_tell", ""),
                        _ => (),
                    }
                }
                return Ok(false);
            }
            self.seekable.set(false);
        }
        context.warning(format!("{function}(): Stream does not support seeking"));
        Ok(false)
    }

    pub async fn stat(
        &self,
        context: &mut dyn Context,
        function: &str,
    ) -> Result<Option<Value>, NativeError> {
        match self.invoke(context, "stream_stat", Vec::new()).await? {
            Some(value @ Value::Array(_)) => Ok(Some(value)),
            None => {
                missing(context, &self.object, function, "stream_stat", "");
                Ok(None)
            }
            _ => Ok(None),
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
