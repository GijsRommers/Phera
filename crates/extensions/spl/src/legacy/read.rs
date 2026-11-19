//! Parse the values in an SPL wire-format stream with an explicit container stack.
//!
//! ```php
//! $array->unserialize('x:i:0;a:1:{i:0;i:7;};m:a:0:{}');
//! ```

use std::{collections::HashMap, rc::Rc};

use phera_extension_api::{ArrayKey, Context, NativeError, Object, PhpArray, Value};

pub(crate) struct Reader<'a> {
    input: &'a [u8],
    offset: usize,
    sequence: usize,
    objects: HashMap<usize, Rc<Object>>,
}

enum Layout {
    Array,
    Object(Rc<Object>),
}

struct Container {
    layout: Layout,
    remaining: usize,
    key: Option<ArrayKey>,
    values: PhpArray,
}

enum Token {
    Value(Value),
    Open(Container),
}

impl<'a> Reader<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            offset: 0,
            sequence: 0,
            objects: HashMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.offset == self.input.len()
    }

    pub fn error(&self) -> NativeError {
        NativeError::throw(
            "UnexpectedValueException",
            format!(
                "Error at offset {} of {} bytes",
                self.offset,
                self.input.len(),
            ),
        )
    }

    pub fn expect(&mut self, bytes: &[u8]) -> Result<(), NativeError> {
        if self.take(bytes) {
            Ok(())
        } else {
            Err(self.error())
        }
    }

    pub fn take(&mut self, bytes: &[u8]) -> bool {
        if self
            .input
            .get(self.offset..)
            .is_some_and(|rest| rest.starts_with(bytes))
        {
            self.offset += bytes.len();
            true
        } else {
            false
        }
    }

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

    fn key(&mut self, layout: &Layout) -> Result<ArrayKey, NativeError> {
        if self.take(b"i:") {
            let index: i64 = self.number(b';')?;
            return Ok(match layout {
                Layout::Array => ArrayKey::Int(index),
                Layout::Object(_) => ArrayKey::str(index.to_string()),
            });
        }
        self.expect(b"s:")?;
        let key = self.string(b"\";")?;
        Ok(ArrayKey::str(key))
    }

    fn token(&mut self, context: &mut dyn Context) -> Result<Token, NativeError> {
        self.sequence = self.sequence.saturating_add(1);
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
            let object = self.objects.get(&number).ok_or_else(|| self.error())?;
            return Ok(Token::Value(Value::Object(object.clone())));
        }
        let layout = if self.take(b"a:") {
            Layout::Array
        } else if self.take(b"O:") {
            let class = self.string(b"\":")?;
            let class = std::str::from_utf8(class).map_err(|_| self.error())?;
            let Value::Object(object) = context.create_object(class, PhpArray::new())? else {
                return Err(self.error());
            };
            if object.class.has_native_properties()
                || object.class.is_instance_of("WeakReference")
                || object.class.is_instance_of("Closure")
                || context.is_user_method(&object, "__unserialize")
                || context.is_user_method(&object, "__wakeup")
            {
                return Err(NativeError::throw(
                    "UnexpectedValueException",
                    format!("{} uses custom serialization", object.class.name(),),
                ));
            }
            self.objects.insert(self.sequence, object.clone());
            Layout::Object(object)
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

    pub fn value(&mut self, context: &mut dyn Context) -> Result<Value, NativeError> {
        let mut stack: Vec<Container> = Vec::new();
        loop {
            let next = if stack.last().is_some_and(|frame| frame.remaining == 0) {
                let frame = stack.pop().ok_or_else(|| self.error())?;
                self.expect(b"}")?;
                match frame.layout {
                    Layout::Array => Value::from(frame.values),
                    Layout::Object(object) => {
                        object
                            .update(|props| {
                                for (key, value) in frame.values.iter() {
                                    props.insert(key, value.clone());
                                }
                            })
                            .ok_or_else(crate::busy)?;
                        Value::Object(object)
                    }
                }
            } else {
                if let Some(frame) = stack.last_mut() {
                    frame.key = Some(self.key(&frame.layout)?);
                }
                match self.token(context)? {
                    Token::Value(value) => value,
                    Token::Open(frame) => {
                        stack.push(frame);
                        continue;
                    }
                }
            };
            let Some(parent) = stack.last_mut() else {
                return Ok(next);
            };
            let key = parent.key.take().ok_or_else(|| self.error())?;
            parent.values.insert(key, next);
            parent.remaining -= 1;
        }
    }
}
