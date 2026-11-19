//! Parse the values in an SPL wire-format stream with an explicit container stack.
//!
//! ```php
//! $array->unserialize('x:i:0;a:1:{i:0;i:7;};m:a:0:{}');
//! ```

use std::rc::Rc;

use phera_extension_api::serialization::{
    LegacyUnserializeState, UnserializeHook, UnserializePolicy,
};
use phera_extension_api::{ArrayKey, Context, NativeError, Object, PhpArray, Value};

pub(crate) struct Reader<'a> {
    pub(super) input: &'a [u8],
    pub(super) offset: usize,
    stack: Vec<Container>,
    pub(super) policy: UnserializePolicy,
    function: &'static str,
}

pub(super) enum Layout {
    Array,
    Object(Rc<Object>),
    Magic(Rc<Object>),
}

pub(super) struct Container {
    pub(super) layout: Layout,
    pub(super) remaining: usize,
    pub(super) key: Option<ArrayKey>,
    pub(super) values: PhpArray,
}

pub(super) enum Token<'a> {
    Value(Value),
    Open(Container),
    Custom(Custom<'a>),
}

pub(super) struct Custom<'a> {
    pub class: &'a [u8],
    pub payload: &'a [u8],
    pub offset: usize,
    pub slot: usize,
    pub policy: UnserializePolicy,
}

pub(super) enum Step<'a> {
    Value(Value),
    Custom(Custom<'a>),
}

impl<'a> Reader<'a> {
    pub fn new(input: &'a [u8], function: &'static str, policy: UnserializePolicy) -> Self {
        Self {
            input,
            offset: 0,
            stack: Vec::new(),
            policy,
            function,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.offset == self.input.len()
    }

    pub fn error(&self) -> NativeError {
        self.error_at(self.offset)
    }

    pub fn error_at(&self, offset: usize) -> NativeError {
        NativeError::throw(
            "UnexpectedValueException",
            format!("Error at offset {} of {} bytes", offset, self.input.len(),),
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

    pub(super) async fn value(
        &mut self,
        context: &mut dyn Context,
        references: &mut LegacyUnserializeState,
        mut completed: Option<Value>,
    ) -> Result<Step<'a>, NativeError> {
        loop {
            let next = if let Some(value) = completed.take() {
                value
            } else if self.stack.last().is_some_and(|frame| frame.remaining == 0) {
                let frame = self.stack.pop().ok_or_else(|| self.error())?;
                self.expect(b"}")?;
                match frame.layout {
                    Layout::Array => Value::from(frame.values),
                    Layout::Object(object) => {
                        context.finish_unserialized_object(&object, frame.values)?;
                        if context.has_serialization_method(&object, "__wakeup") {
                            references
                                .hooks
                                .push(UnserializeHook::Wakeup(object.clone()));
                        }
                        Value::Object(object)
                    }
                    Layout::Magic(object) => {
                        references
                            .hooks
                            .push(UnserializeHook::Unserialize(object.clone(), frame.values));
                        Value::Object(object)
                    }
                }
            } else {
                self.next_key(context)?;
                let depth = self.stack.len();
                match self.token(context, references, depth).await? {
                    Token::Value(value) => value,
                    Token::Open(frame) => {
                        if !(frame.remaining == 0 && matches!(frame.layout, Layout::Array))
                            && self.policy.depth_exceeded(depth)
                        {
                            context.warning(self.policy.depth_warning(self.function));
                            return Err(self.error());
                        }
                        self.stack.push(frame);
                        continue;
                    }
                    Token::Custom(custom) => return Ok(Step::Custom(custom)),
                }
            };
            let Some(parent) = self.stack.last_mut() else {
                return Ok(Step::Value(next));
            };
            if let Some(key) = parent.key.take() {
                parent.values.insert(key, next);
                parent.remaining -= 1;
            } else {
                return Err(self.error());
            }
        }
    }

    fn next_key(&mut self, context: &mut dyn Context) -> Result<(), NativeError> {
        // A key advances the reader, so own the frame while decoding it.
        let Some(mut frame) = self.stack.pop() else {
            return Ok(());
        };
        let key = self.key(&frame.layout)?;
        let key = match &frame.layout {
            Layout::Object(object) => {
                let key = context
                    .unserialized_property_key(object, key)?
                    .ok_or_else(|| self.error())?;
                if !frame.values.contains_key(&key) && !object.has(&key) {
                    context.unserialized_dynamic_property(object, &key)?;
                }
                key
            }
            _ => key,
        };
        frame.key = Some(key);
        self.stack.push(frame);
        Ok(())
    }
}
