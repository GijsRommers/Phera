//! Delegation forwards keys without changing the outer generator's automatic key.
//!
//! ```php
//! function numbers() { yield from [4 => 10]; yield 20; } // keys 4, then 0
//! ```

use std::rc::Rc;

use phera_extension_api::{
    NativeError,
    traversal::{self, IteratorStep, Traversal},
};
use phera_value::{ArrayKey, Object, PhpArray, Value, to_bool};

use crate::Vm;

use super::{Item, generator_object};

impl Delegate {
    pub(super) fn forked(&self, fork: &mut phera_value::Fork<'_>) -> Result<Self, String> {
        match self {
            Delegate::Array { values, position } => Ok(Delegate::Array {
                values: fork.array(values),
                position: *position,
            }),
            Delegate::Generator(inner) => Ok(Delegate::Generator(fork.object(inner))),
            Delegate::Cursor { .. } => Err("it delegates to an iterator with yield from".into()),
        }
    }
}

pub(super) enum Delegate {
    Array {
        values: Rc<PhpArray>,
        position: Option<usize>,
    },
    Cursor {
        cursor: Box<dyn Traversal>,
        operation: CursorPosition,
    },
    Generator(Rc<Object>),
}

pub(super) enum CursorPosition {
    First,
    Next,
}

impl Vm {
    pub(super) async fn generator_delegate_source(
        &mut self,
        source: Value,
    ) -> Result<Delegate, NativeError> {
        match source {
            Value::Array(values) => {
                let position = values.first_position();
                Ok(Delegate::Array { values, position })
            }
            Value::Object(object) if generator_object(&object) => Ok(Delegate::Generator(object)),
            Value::Object(object) if object.class.is_traversable() => {
                let mut cursor = self.make_traversal(object, false).await?;
                cursor.step(self, IteratorStep::Rewind).await?;
                Ok(Delegate::Cursor {
                    cursor,
                    operation: CursorPosition::First,
                })
            }
            _ => Err(NativeError::error(
                "Can use \"yield from\" only with arrays and Traversables",
            )),
        }
    }

    pub(super) async fn poll_generator_delegate(
        &mut self,
        delegate: &mut Delegate,
    ) -> Result<Option<Item>, NativeError> {
        match delegate {
            Delegate::Array { values, position } => {
                let Some(index) = *position else {
                    return Ok(None);
                };
                *position = values.next_position(index);
                Ok(values.entry_at(index).map(|(key, value)| Item {
                    key: match key {
                        ArrayKey::Int(key) => Value::Int(key),
                        ArrayKey::String(key) => Value::String(key),
                    },
                    value: value.clone(),
                }))
            }
            Delegate::Cursor { cursor, operation } => {
                if matches!(operation, CursorPosition::Next) {
                    cursor.step(self, IteratorStep::Advance).await?;
                }
                *operation = CursorPosition::Next;
                if !to_bool(&cursor.step(self, IteratorStep::Valid).await?) {
                    return Ok(None);
                }
                let (value, key) = traversal::current(cursor.as_mut(), self, true).await?;
                Ok(Some(Item {
                    key: key.unwrap_or_default(),
                    value,
                }))
            }
            Delegate::Generator(_) => Err(internal_error!("generator delegate cursor")),
        }
    }
}
