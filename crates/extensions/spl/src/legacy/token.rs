//! Read one PHP wire token, such as `i:42;` or `C:11:"ArrayObject":...`.

use phera_extension_api::serialization::{
    INCOMPLETE_CLASS, INCOMPLETE_CLASS_NAME, LegacyUnserializeState, resolve_unserialize_class,
};
use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value};

use super::read::{Container, Custom, Layout, Reader, Token};

impl<'a> Reader<'a> {
    fn until(&mut self, delimiter: u8) -> Result<&'a [u8], NativeError> {
        let rest = self.input.get(self.offset..).ok_or_else(|| self.error())?;
        let end = rest
            .iter()
            .position(|byte| *byte == delimiter)
            .ok_or_else(|| self.error())?;
        self.offset += end + 1;
        Ok(&rest[..end])
    }

    fn number<T: std::str::FromStr>(&mut self, delimiter: u8) -> Result<T, NativeError> {
        let bytes = self.until(delimiter)?;
        std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| text.parse().ok())
            .ok_or_else(|| self.error())
    }

    fn string(&mut self, close: &[u8]) -> Result<&'a [u8], NativeError> {
        let len: usize = self.number(b':')?;
        self.expect(b"\"")?;
        let end = self.offset.checked_add(len).ok_or_else(|| self.error())?;
        let bytes = self
            .input
            .get(self.offset..end)
            .ok_or_else(|| self.error())?;
        self.offset = end;
        self.expect(close)?;
        Ok(bytes)
    }

    pub(super) fn key(&mut self, layout: &Layout) -> Result<ArrayKey, NativeError> {
        if self.take(b"i:") {
            let index: i64 = self.number(b';')?;
            return Ok(match layout {
                Layout::Array | Layout::Magic(_) => ArrayKey::Int(index),
                Layout::Object(_) => ArrayKey::str(index.to_string()),
            });
        }
        self.expect(b"s:")?;
        let key = self.string(b"\";")?;
        Ok(ArrayKey::str(key))
    }

    pub(super) async fn token(
        &mut self,
        context: &mut dyn Context,
        references: &mut LegacyUnserializeState,
        depth: usize,
    ) -> Result<Token<'a>, NativeError> {
        references.count = references.count.saturating_add(1);
        if self.take(b"N;") {
            return Ok(Token::Value(Value::Null));
        }
        if self.take(b"b:") {
            let value = if self.take(b"0;") {
                false
            } else {
                self.expect(b"1;")?;
                true
            };
            return Ok(Token::Value(Value::Bool(value)));
        }
        if self.take(b"i:") {
            return Ok(Token::Value(Value::Int(self.number(b';')?)));
        }
        if self.take(b"d:") {
            let value = if self.take(b"NAN;") {
                f64::NAN
            } else if self.take(b"INF;") {
                f64::INFINITY
            } else if self.take(b"-INF;") {
                f64::NEG_INFINITY
            } else {
                self.number(b';')?
            };
            return Ok(Token::Value(Value::Float(value)));
        }
        if self.take(b"s:") {
            return Ok(Token::Value(Value::from(self.string(b"\";")?.to_vec())));
        }
        if self.take(b"r:") {
            let number = self.number(b';')?;
            let object = references
                .objects
                .get(&number)
                .ok_or_else(|| self.error())?;
            return Ok(Token::Value(Value::Object(object.clone())));
        }
        let layout = if self.take(b"a:") {
            Layout::Array
        } else if self.take(b"O:") {
            let class = self.string(b"\":")?;
            let info = resolve_unserialize_class(context, class, &self.policy).await?;
            let name = info
                .as_ref()
                .map_or(INCOMPLETE_CLASS, |class| class.name.as_str());
            let Value::Object(object) = context.create_unserialized_object(name)? else {
                return Err(self.error());
            };
            if info.is_none() {
                object.set(ArrayKey::str(INCOMPLETE_CLASS_NAME), Value::from(class));
            }
            let magic = info.as_ref().is_some_and(|class| class.has_unserialize);
            if !magic
                && (object.class.has_native_properties()
                    || context.serialization_forbidden(&object))
            {
                return Err(NativeError::throw(
                    "UnexpectedValueException",
                    format!("{} uses custom serialization", object.class.name()),
                ));
            }
            references.objects.insert(references.count, object.clone());
            if magic {
                Layout::Magic(object)
            } else {
                Layout::Object(object)
            }
        } else if self.take(b"C:") {
            return self
                .custom_object(references.count, depth)
                .map(Token::Custom);
        } else {
            return Err(self.error());
        };
        let remaining = self.number(b':')?;
        self.expect(b"{")?;
        Ok(Token::Open(Container {
            layout,
            remaining,
            key: None,
            values: PhpArray::new(),
        }))
    }

    fn custom_object(&mut self, slot: usize, depth: usize) -> Result<Custom<'a>, NativeError> {
        let class = self.string(b"\":")?;
        let length: usize = self.number(b':')?;
        self.expect(b"{")?;
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| self.error())?;
        let payload = self
            .input
            .get(self.offset..end)
            .ok_or_else(|| self.error())?;
        if self.input.get(end) != Some(&b'}') {
            return Err(self.error());
        }
        let offset = self.offset;
        self.offset = end + 1;
        let mut policy = self.policy.clone();
        policy.depth = policy.depth.saturating_add(depth);
        Ok(Custom {
            class,
            payload,
            offset,
            slot,
            policy,
        })
    }
}
