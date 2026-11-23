//! `session_encode()` uses either `key|value`, one array, or length-prefixed keys.

use phera_ext_serialize::{Decoded, SequenceEncoder, decode_entries, decode_value};
use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value};

use crate::{id::c_string, settings, state::warning};

#[derive(Clone, Copy)]
pub enum Format {
    Text,
    Array,
    Binary,
}

impl Format {
    pub fn current(context: &mut dyn Context) -> Self {
        let name = settings::text(context, "serialize_handler");
        let name = c_string(name.as_bytes());
        if name.eq_ignore_ascii_case(b"php_serialize") {
            Self::Array
        } else if name.eq_ignore_ascii_case(b"php_binary") {
            Self::Binary
        } else {
            Self::Text
        }
    }

    pub async fn encode(
        self,
        context: &mut dyn Context,
        variables: Value,
        function: &str,
    ) -> Result<Option<Vec<u8>>, NativeError> {
        let Value::Array(array) = variables else {
            return Ok(None);
        };
        let mut encoder = SequenceEncoder::default();
        if matches!(self, Self::Array) {
            encoder.push(context, b"", Value::Array(array)).await?;
            return Ok(Some(encoder.finish()));
        }
        for (key, value) in array.iter() {
            let key = match key {
                ArrayKey::String(key) => key,
                ArrayKey::Int(key) => {
                    warning(context, function, &format!("Skipping numeric key {key}"));
                    continue;
                }
            };
            let key = key.as_bytes();
            let mut prefix = Vec::new();
            match self {
                Self::Text if key.contains(&b'|') => {
                    let key = String::from_utf8_lossy(c_string(key));
                    warning(
                        context,
                        function,
                        &format!(
                            "Failed to write session data. Data contains invalid key \"{key}\""
                        ),
                    );
                    return Ok(None);
                }
                Self::Text => {
                    prefix.extend_from_slice(key);
                    prefix.push(b'|');
                }
                Self::Binary if key.len() > 127 => continue,
                Self::Binary => {
                    prefix.push(key.len() as u8);
                    prefix.extend_from_slice(key);
                }
                Self::Array => continue,
            }
            encoder.push(context, &prefix, value.clone()).await?;
        }
        let bytes = encoder.finish();
        Ok((!bytes.is_empty()).then_some(bytes))
    }

    pub async fn decode(
        self,
        context: &mut dyn Context,
        bytes: &[u8],
        function: &str,
        publish: fn(&mut dyn Context, Value) -> Result<(), NativeError>,
    ) -> Result<Decoded, NativeError> {
        match self {
            Self::Array if bytes.is_empty() => {
                publish(context, Value::from(PhpArray::new()))?;
                Ok(Decoded::Complete)
            }
            Self::Array => decode_value(context, bytes, function, publish).await,
            Self::Text => {
                decode_entries(
                    context,
                    bytes,
                    function,
                    |rest| {
                        let end = rest.iter().position(|byte| *byte == b'|')?;
                        Some((rest[..end].to_vec().into(), end + 1))
                    },
                    publish,
                )
                .await
            }
            Self::Binary => {
                decode_entries(
                    context,
                    bytes,
                    function,
                    |rest| {
                        let length = usize::from(*rest.first()? & 0x7f);
                        let key = rest.get(1..=length)?;
                        Some((key.to_vec().into(), length + 1))
                    },
                    publish,
                )
                .await
            }
        }
    }
}
