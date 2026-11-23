//! PHP values and their semantics: strings, arrays, conversions, comparison
//! and arithmetic. See `docs/architecture.md`, section 3.

mod array;
mod attachments;
mod convert;
mod cycles;
mod destruction;
mod error;
mod float_format;
mod fork;
mod integer_text;
mod lazy_object;
mod object;
mod ops;
mod properties;
mod string;
mod value;
mod weak_object;

pub use array::{ArrayKey, PhpArray};
pub use convert::{
    NumVal, NumericParse, cast_to_bool, is_numeric, parse_numeric, strtod_prefix, to_array,
    to_bool, to_float, to_int, to_php_string,
};
pub use cycles::{Garbage, find_garbage};
pub use destruction::{Destruction, Doomed, ResourceRelease};
pub use error::{
    CollectDiagnostics, DiagnosticKind, Diagnostics, ErrorKind, NoDiagnostics, OpError,
};
pub use float_format::{
    float_repr, float_repr_with_precision, float_to_string_precision14, with_float_repr,
};
pub use fork::{Fork, NativeFork, Refusal};
pub use integer_text::{integer_string, push_integer};
pub use lazy_object::{LazyObject, LazyStrategy};
pub use object::{
    ClassHandle, Object, ObjectCast, ObjectIds, PropertyScope, Resource, class_display_name,
    split_property_key,
};
pub use ops::compare::{smart_strcmp, threeway};
pub use ops::{
    ConvFail, LazyComparison, ObjectComparator, ObjectComparison, ObjectOperands, StringComparison,
    StringComparisonStep, add, binop_type_error, bitwise_and, bitwise_not, bitwise_or, bitwise_xor,
    compare, compare_after_error, compare_op, compare_with_objects, compare_with_string_objects,
    concat, decrement, div, increment, less, less_equal, loose_equals, loose_equals_with_objects,
    modulo, mul, negate, pow, shift_left, shift_right, strict_equals, sub, try_get_long,
    unary_plus,
};
pub use properties::{Properties, PropertyLayout};
pub use string::PhpString;
pub use value::{Value, ValueRef};
pub use weak_object::WeakObject;
