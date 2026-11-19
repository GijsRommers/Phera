//! The set functions with callbacks: `array_udiff`, `array_diff_ukey`,
//! `array_uintersect_uassoc` and the rest, which keep the first array's
//! keys.
//!
//! Except where keys are compared exactly (the `_assoc` forms with a value
//! callback only), PHP sorts every array with the callbacks and then walks
//! them side by side (callback_set_merge.rs), so the callbacks see the same
//! comparisons as in PHP.

use std::cmp::Ordering;
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    PhpArray, Value,
};

use super::callback::{BoolResult, UserComparison};

mod comparison;
mod merge;

use comparison::SetComparison;

/// A key and its value, as the sorted copies of the arrays hold them.
pub(super) type Entry = (ArrayKey, Value);

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    ARRAY_UDIFF,
    ARRAY_UDIFF_ASSOC,
    ARRAY_UDIFF_UASSOC,
    ARRAY_DIFF_UKEY,
    ARRAY_DIFF_UASSOC,
    ARRAY_UINTERSECT,
    ARRAY_UINTERSECT_ASSOC,
    ARRAY_UINTERSECT_UASSOC,
    ARRAY_INTERSECT_UKEY,
    ARRAY_INTERSECT_UASSOC,
];

/// What one of the ten functions computes and compares.
#[derive(Clone, Copy)]
struct Variant {
    function: &'static str,
    result: SetResult,
    values: Values,
    keys: Keys,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SetResult {
    Difference,
    Intersection,
}

/// How values are compared: with the first callback, as strings, or not.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Values {
    Callback,
    Strings,
    Ignored,
}

/// How keys are compared: with the last callback, with `===`, or not.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Keys {
    Callback,
    Exact,
    Ignored,
}

/// `array_udiff(array $array, array ...$arrays, callable ...$callbacks): array`,
/// the entries whose value no other array has, by a value callback.
///
/// ```php
/// array_udiff([1, 5, 3], [3], fn($a, $b) => $a <=> $b); // [0 => 1, 1 => 5]
/// ```
const ARRAY_UDIFF: FunctionEntry = FunctionEntry::asynchronous(
    "array_udiff",
    // PHP checks the callbacks at the end before the arrays, and needs one
    // more argument per callback; run() does all of it, in that order.
    &[
        Param::new("array", ParamType::Mixed),
        Param::new("rest", ParamType::Mixed).variadic(),
    ],
    array_udiff,
)
.with_handler_arity();

fn array_udiff(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    let variant = Variant {
        function: "array_udiff",
        result: SetResult::Difference,
        values: Values::Callback,
        keys: Keys::Ignored,
    };
    Box::pin(run(context, arguments, variant))
}

/// `array_udiff_assoc(array $array, array ...$arrays, callable ...$callbacks): array`,
/// by a value callback, keys compared exactly.
const ARRAY_UDIFF_ASSOC: FunctionEntry = FunctionEntry::asynchronous(
    "array_udiff_assoc",
    // PHP checks the callbacks at the end before the arrays, and needs one
    // more argument per callback; run() does all of it, in that order.
    &[
        Param::new("array", ParamType::Mixed),
        Param::new("rest", ParamType::Mixed).variadic(),
    ],
    array_udiff_assoc,
)
.with_handler_arity();

fn array_udiff_assoc(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    let variant = Variant {
        function: "array_udiff_assoc",
        result: SetResult::Difference,
        values: Values::Callback,
        keys: Keys::Exact,
    };
    Box::pin(run(context, arguments, variant))
}

/// `array_udiff_uassoc(array $array, array ...$arrays, callable ...$callbacks): array`,
/// by a value callback and a key callback.
const ARRAY_UDIFF_UASSOC: FunctionEntry = FunctionEntry::asynchronous(
    "array_udiff_uassoc",
    // PHP checks the callbacks at the end before the arrays, and needs one
    // more argument per callback; run() does all of it, in that order.
    &[
        Param::new("array", ParamType::Mixed),
        Param::new("rest", ParamType::Mixed).variadic(),
    ],
    array_udiff_uassoc,
)
.with_handler_arity();

fn array_udiff_uassoc(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    let variant = Variant {
        function: "array_udiff_uassoc",
        result: SetResult::Difference,
        values: Values::Callback,
        keys: Keys::Callback,
    };
    Box::pin(run(context, arguments, variant))
}

/// `array_diff_ukey(array $array, array ...$arrays, callable ...$callbacks): array`,
/// the entries whose key no other array has, by a key callback.
const ARRAY_DIFF_UKEY: FunctionEntry = FunctionEntry::asynchronous(
    "array_diff_ukey",
    // PHP checks the callbacks at the end before the arrays, and needs one
    // more argument per callback; run() does all of it, in that order.
    &[
        Param::new("array", ParamType::Mixed),
        Param::new("rest", ParamType::Mixed).variadic(),
    ],
    array_diff_ukey,
)
.with_handler_arity();

fn array_diff_ukey(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    let variant = Variant {
        function: "array_diff_ukey",
        result: SetResult::Difference,
        values: Values::Ignored,
        keys: Keys::Callback,
    };
    Box::pin(run(context, arguments, variant))
}

/// `array_diff_uassoc(array $array, array ...$arrays, callable ...$callbacks): array`,
/// values compared as strings, keys by a callback.
const ARRAY_DIFF_UASSOC: FunctionEntry = FunctionEntry::asynchronous(
    "array_diff_uassoc",
    // PHP checks the callbacks at the end before the arrays, and needs one
    // more argument per callback; run() does all of it, in that order.
    &[
        Param::new("array", ParamType::Mixed),
        Param::new("rest", ParamType::Mixed).variadic(),
    ],
    array_diff_uassoc,
)
.with_handler_arity();

fn array_diff_uassoc(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    let variant = Variant {
        function: "array_diff_uassoc",
        result: SetResult::Difference,
        values: Values::Strings,
        keys: Keys::Callback,
    };
    Box::pin(run(context, arguments, variant))
}

/// `array_uintersect(array $array, array ...$arrays, callable ...$callbacks): array`,
/// the entries whose value every other array has, by a value callback.
const ARRAY_UINTERSECT: FunctionEntry = FunctionEntry::asynchronous(
    "array_uintersect",
    // PHP checks the callbacks at the end before the arrays, and needs one
    // more argument per callback; run() does all of it, in that order.
    &[
        Param::new("array", ParamType::Mixed),
        Param::new("rest", ParamType::Mixed).variadic(),
    ],
    array_uintersect,
)
.with_handler_arity();

fn array_uintersect(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    let variant = Variant {
        function: "array_uintersect",
        result: SetResult::Intersection,
        values: Values::Callback,
        keys: Keys::Ignored,
    };
    Box::pin(run(context, arguments, variant))
}

/// `array_uintersect_assoc(array $array, array ...$arrays, callable ...$callbacks): array`,
/// by a value callback, keys compared exactly.
const ARRAY_UINTERSECT_ASSOC: FunctionEntry = FunctionEntry::asynchronous(
    "array_uintersect_assoc",
    // PHP checks the callbacks at the end before the arrays, and needs one
    // more argument per callback; run() does all of it, in that order.
    &[
        Param::new("array", ParamType::Mixed),
        Param::new("rest", ParamType::Mixed).variadic(),
    ],
    array_uintersect_assoc,
)
.with_handler_arity();

fn array_uintersect_assoc(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    let variant = Variant {
        function: "array_uintersect_assoc",
        result: SetResult::Intersection,
        values: Values::Callback,
        keys: Keys::Exact,
    };
    Box::pin(run(context, arguments, variant))
}

/// `array_uintersect_uassoc(array $array, array ...$arrays, callable ...$callbacks): array`,
/// by a value callback and a key callback.
const ARRAY_UINTERSECT_UASSOC: FunctionEntry = FunctionEntry::asynchronous(
    "array_uintersect_uassoc",
    // PHP checks the callbacks at the end before the arrays, and needs one
    // more argument per callback; run() does all of it, in that order.
    &[
        Param::new("array", ParamType::Mixed),
        Param::new("rest", ParamType::Mixed).variadic(),
    ],
    array_uintersect_uassoc,
)
.with_handler_arity();

fn array_uintersect_uassoc(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    let variant = Variant {
        function: "array_uintersect_uassoc",
        result: SetResult::Intersection,
        values: Values::Callback,
        keys: Keys::Callback,
    };
    Box::pin(run(context, arguments, variant))
}

/// `array_intersect_ukey(array $array, array ...$arrays, callable ...$callbacks): array`,
/// the entries whose key every other array has, by a key callback.
const ARRAY_INTERSECT_UKEY: FunctionEntry = FunctionEntry::asynchronous(
    "array_intersect_ukey",
    // PHP checks the callbacks at the end before the arrays, and needs one
    // more argument per callback; run() does all of it, in that order.
    &[
        Param::new("array", ParamType::Mixed),
        Param::new("rest", ParamType::Mixed).variadic(),
    ],
    array_intersect_ukey,
)
.with_handler_arity();

fn array_intersect_ukey(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    let variant = Variant {
        function: "array_intersect_ukey",
        result: SetResult::Intersection,
        values: Values::Ignored,
        keys: Keys::Callback,
    };
    Box::pin(run(context, arguments, variant))
}

/// `array_intersect_uassoc(array $array, array ...$arrays, callable ...$callbacks): array`,
/// values compared as strings, keys by a callback.
const ARRAY_INTERSECT_UASSOC: FunctionEntry = FunctionEntry::asynchronous(
    "array_intersect_uassoc",
    // PHP checks the callbacks at the end before the arrays, and needs one
    // more argument per callback; run() does all of it, in that order.
    &[
        Param::new("array", ParamType::Mixed),
        Param::new("rest", ParamType::Mixed).variadic(),
    ],
    array_intersect_uassoc,
)
.with_handler_arity();

fn array_intersect_uassoc(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    let variant = Variant {
        function: "array_intersect_uassoc",
        result: SetResult::Intersection,
        values: Values::Strings,
        keys: Keys::Callback,
    };
    Box::pin(run(context, arguments, variant))
}

pub(super) fn intersect_strings(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    let variant = Variant {
        function: "array_intersect",
        result: SetResult::Intersection,
        values: Values::Strings,
        keys: Keys::Ignored,
    };
    run_builtin_string_set(context, arguments, variant)
}

pub(super) fn intersect_associative_strings(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    run_builtin_string_set(
        context,
        arguments,
        Variant {
            function: "array_intersect_assoc",
            result: SetResult::Intersection,
            values: Values::Strings,
            keys: Keys::Exact,
        },
    )
}

pub(super) fn diff_associative_strings(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    run_builtin_string_set(
        context,
        arguments,
        Variant {
            function: "array_diff_assoc",
            result: SetResult::Difference,
            values: Values::Strings,
            keys: Keys::Exact,
        },
    )
}

fn run_builtin_string_set(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    variant: Variant,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        run_set(context, &arguments, variant)
            .await
            .map_err(|error| context.native_call_error(variant.function, &arguments, error))
    })
}

async fn run(context: &mut dyn Context, arguments: Vec<Value>, variant: Variant) -> NativeResult {
    run_set(context, &arguments, variant).await
}

async fn run_set(context: &mut dyn Context, arguments: &[Value], variant: Variant) -> NativeResult {
    let name = variant.function;
    let user_data = variant.values == Values::Callback;
    let user_keys = variant.keys == Keys::Callback;
    let callback_count = usize::from(user_data) + usize::from(user_keys);
    let minimum = callback_count + 1;
    if arguments.len() < minimum {
        return Err(NativeError::argument_count_error(format!(
            "{name}() expects at least {minimum} arguments, {} given",
            arguments.len()
        )));
    }
    let array_count = arguments.len() - callback_count;
    for (index, callback) in arguments.iter().enumerate().skip(array_count) {
        super::callback::check_trailing_callback(name, index + 1, callback)?;
    }
    let mut comparison = SetComparison {
        comparison: UserComparison {
            function: name,
            deprecated: false,
        },
        data: user_data.then(|| &arguments[array_count]),
        keys: user_keys.then(|| &arguments[arguments.len() - 1]),
        key_only: variant.values == Values::Ignored,
    };
    let exact_keys = variant.keys == Keys::Exact;
    let intersection = variant.result == SetResult::Intersection;
    let mut arrays = Vec::new();
    let mut sorted = Vec::new();
    for (index, value) in arguments.iter().take(array_count).enumerate() {
        let Value::Array(array) = value else {
            let parameter = if index == 0 { " ($array)" } else { "" };
            return Err(NativeError::type_error(format!(
                "{name}(): Argument #{}{parameter} must be of type array, {} given",
                index + 1,
                crate::arguments::given_type(value)
            )));
        };
        arrays.push(array.clone());
        if !exact_keys {
            let entries: Vec<Entry> = array
                .iter()
                .map(|(key, value)| (key, value.clone()))
                .collect();
            let indices = super::zend_sort::order_async(entries.len(), async |left, right| {
                comparison
                    .order(context, &entries[left], &entries[right])
                    .await
            })
            .await?;
            sorted.push(
                indices
                    .into_iter()
                    .map(|index| entries[index].clone())
                    .collect(),
            );
        }
    }
    if exact_keys {
        exact(context, &arrays, &mut comparison, intersection).await
    } else {
        merge::run(context, &arrays[0], &sorted, &mut comparison, intersection).await
    }
}

/// The *_assoc variants with built-in key equality use hash lookup, without sorting.
async fn exact(
    context: &mut dyn Context,
    arrays: &[Rc<PhpArray>],
    comparison: &mut SetComparison<'_>,
    intersection: bool,
) -> NativeResult {
    let mut result = PhpArray::new();
    for (key, value) in arrays[0].iter() {
        let mut keep = true;
        for array in arrays.iter().skip(1) {
            let found = if let Some(other) = array.get(&key) {
                comparison
                    .data(
                        context,
                        &(key.clone(), value.clone()),
                        &(key.clone(), other.clone()),
                        BoolResult::Number,
                    )
                    .await?
                    == Ordering::Equal
            } else {
                false
            };
            if found != intersection {
                keep = false;
                break;
            }
        }
        if keep {
            result.insert(key, value.clone());
        }
    }
    Ok(Value::from(result))
}
