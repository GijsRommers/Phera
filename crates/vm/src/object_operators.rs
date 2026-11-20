//! Native numeric objects may implement the same PHP operators as scalars.
//! For example, `1 + new BcMath\Number('0.20')` preserves decimal precision.

use phera_bytecode::{BinaryOp, UnaryOp};
use phera_extension_api::{NativeBinary, NativeError, Value};

use crate::{Vm, classes::Class};

#[inline]
pub(crate) fn has_binary(left: &Value, right: &Value) -> bool {
    matches!(left, Value::Object(_)) && binary_hook(left).is_some()
        || matches!(right, Value::Object(_)) && binary_hook(right).is_some()
}

#[cold]
fn binary_hook(value: &Value) -> Option<NativeBinary> {
    let Value::Object(object) = value else {
        return None;
    };
    object
        .class
        .as_any()
        .downcast_ref::<Class>()?
        .hooks
        .as_ref()?
        .binary
}

impl Vm {
    pub(crate) fn native_unary(
        &mut self,
        operator: UnaryOp,
        value: &Value,
    ) -> Result<Option<Value>, NativeError> {
        let value = value.deref();
        let Value::Object(object) = &*value else {
            return Ok(None);
        };
        let Some(handler) = object
            .class
            .as_any()
            .downcast_ref::<Class>()
            .and_then(|class| class.hooks.as_ref())
            .and_then(|hooks| hooks.unary)
        else {
            return Ok(None);
        };
        handler(self, operator, object)
    }

    #[cold]
    pub(crate) fn native_binary(
        &mut self,
        operator: BinaryOp,
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>, NativeError> {
        let left = left.deref();
        let right = right.deref();
        for hook in [binary_hook(&left), binary_hook(&right)]
            .into_iter()
            .flatten()
        {
            if let Some(result) = hook(self, operator, &left, &right)? {
                return Ok(Some(result));
            }
        }
        Ok(None)
    }
}
