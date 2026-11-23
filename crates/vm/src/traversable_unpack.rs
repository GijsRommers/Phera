//! Unpacking pulls one item at a time so key or call errors end traversal.
//!
//! ```php
//! $array = [...$items];
//! format(...$items);
//! ```

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::Reg;
use phera_extension_api::{
    NativeError,
    traversal::{self, IteratorStep},
};
use phera_value::{ArrayKey, Value, to_bool};

use crate::{Vm, calls::call_view::CallView};

#[derive(Clone, Copy)]
pub(crate) enum SpreadTarget {
    Array(Reg),
    Arguments,
}

impl Vm {
    pub(crate) async fn spread_traversable(
        &mut self,
        source: Reg,
        target: SpreadTarget,
    ) -> Result<(), NativeError> {
        let Value::Object(object) = self.get(source) else {
            return Err(internal_error!("Traversable spread source"));
        };
        if crate::generators::generator_object(&object) {
            self.discard_generator_temporary(source);
        }
        let mut cursor = self.make_traversal(object, false).await?;
        cursor.step(self, IteratorStep::Rewind).await?;
        while to_bool(&cursor.step(self, IteratorStep::Valid).await?) {
            let (value, key) = traversal::current(cursor.as_mut(), self, true).await?;
            let key = spread_key(key.unwrap_or_default(), target)?;
            let value = value.deref().clone();
            match target {
                SpreadTarget::Array(register) => {
                    let Some(Value::Array(array)) =
                        self.current_slot_mut(register).and_then(Option::as_mut)
                    else {
                        return Err(internal_error!("Traversable spread array"));
                    };
                    if matches!(key, ArrayKey::Int(_)) {
                        Rc::make_mut(array).push(value)?;
                    } else {
                        Rc::make_mut(array).insert(key, value);
                    }
                }
                SpreadTarget::Arguments => self.send_traversable_argument(key, value)?,
            }
            cursor.step(self, IteratorStep::Advance).await?;
        }
        Ok(())
    }

    fn send_traversable_argument(
        &mut self,
        key: ArrayKey,
        mut value: Value,
    ) -> Result<(), NativeError> {
        let index = self.unpack_index(&key)?;
        let view = CallView::top(&self.code, &self.call_stack)?;
        let parameters = view.parameters();
        if parameters
            .receiving(index)
            .is_some_and(|parameter| parameters.requires_reference(parameter))
        {
            self.warn(format!(
                "Cannot pass by-reference argument {} of {}() by unpacking a Traversable, passing by-value instead",
                index + 1, view.name()
            ));
            value = Value::Reference(Rc::new(RefCell::new(value)));
        }
        self.store_sent_value(index, value)
    }
}

fn spread_key(value: Value, target: SpreadTarget) -> Result<ArrayKey, NativeError> {
    if matches!(
        &*value.deref(),
        Value::Int(_) | Value::InternedStr(_) | Value::String(_)
    ) {
        return Ok(ArrayKey::from_value(&value)?);
    }
    let target = match target {
        SpreadTarget::Array(_) => "array",
        SpreadTarget::Arguments => "argument",
    };
    Err(NativeError::error(format!(
        "Keys must be of type int|string during {target} unpacking"
    )))
}
