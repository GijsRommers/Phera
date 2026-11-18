//! Reading arguments beyond what `phera_extension_api::args` offers.

use std::rc::Rc;

use phera_extension_api::{PhpArray, Value, args};

/// A required `array` parameter.
///
/// `prepare_arguments` runs before every native function and guarantees
/// that a required parameter is there and has its declared type, so the
/// array is always there; a missing one would be a bug in the caller.
pub(crate) fn required_array(arguments: &[Value], index: usize) -> &Rc<PhpArray> {
    args::array(arguments, index).expect("prepare_arguments checks required array parameters")
}

/// A type as PHP names it in an argument error: `true` and `false` rather
/// than `bool`, and the class name for an object.
///
/// ```php
/// max(false); // max(): Argument #1 ($value) must be of type array, false given
/// ```
pub(crate) fn given_type(value: &Value) -> String {
    match value {
        Value::Bool(false) => "false".into(),
        Value::Bool(true) => "true".into(),
        _ => value.type_name(),
    }
}
