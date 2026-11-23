//! Ordered membership and search. `array_search("b", ["x" => "b"])` returns `"x"`.

use phera_extension_api::value::{loose_equals, strict_equals};
use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args,
};

use crate::arguments::required_array;

pub(super) fn register_sync_selectors(registry: &mut Registry) {
    registry.register_sync_selector("in_array", |slots| {
        can_search_sync(slots).then_some(in_array_sync)
    });
    registry.register_sync_selector("array_search", |slots| {
        can_search_sync(slots).then_some(array_search_sync)
    });
}

fn can_search_sync(slots: &[Option<Value>]) -> bool {
    if matches!(slots.get(2), Some(Some(Value::Bool(true)))) {
        return true;
    }
    let [Some(needle), Some(Value::Array(haystack)), ..] = slots else {
        return false;
    };
    if matches!(needle, Value::Object(_) | Value::Reference(_)) {
        return false;
    }
    haystack.values().all(|value| match value {
        Value::Object(_) | Value::Reference(_) => false,
        Value::Array(_) => !matches!(needle, Value::Array(_)),
        _ => true,
    })
}

fn plainly_non_numeric(bytes: &[u8]) -> bool {
    // A numeric string starts with whitespace, a sign, a dot or a digit.
    bytes.first().is_none_or(|first| *first > b'9')
}

fn in_array_sync(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let equality = Equality::from_strict_argument(arguments);
    for value in required_array(arguments, 1).values() {
        if equality.equal_sync(context, &arguments[0], value)? {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

fn array_search_sync(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let equality = Equality::from_strict_argument(arguments);
    for (key, value) in required_array(arguments, 1).iter() {
        if equality.equal_sync(context, &arguments[0], value)? {
            return Ok(Value::from(key));
        }
    }
    Ok(Value::Bool(false))
}

/// How `in_array`, `array_search` and `array_keys` compare.
#[derive(Clone, Copy)]
pub(super) enum Equality {
    /// `==`, with PHP's type juggling.
    Loose,
    /// `===`.
    Strict,
}

impl Equality {
    /// From the `$strict` argument, which is the third in all three.
    pub(super) fn from_strict_argument(arguments: &[Value]) -> Self {
        if args::bool(arguments, 2).unwrap_or(false) {
            Self::Strict
        } else {
            Self::Loose
        }
    }

    pub(super) async fn equal(
        self,
        context: &mut dyn Context,
        left: &Value,
        right: &Value,
    ) -> Result<bool, NativeError> {
        if matches!(self, Self::Loose) && super::ordering::needs_context_comparison(left, right) {
            return Ok(context.compare(left, right).await? == 0);
        }
        self.equal_sync(context, left, right)
    }

    fn equal_sync(
        self,
        context: &mut dyn Context,
        left: &Value,
        right: &Value,
    ) -> Result<bool, NativeError> {
        if let (Some(left), Some(right)) = (left.str_ref(), right.str_ref())
            && (matches!(self, Self::Strict)
                || plainly_non_numeric(left.as_bytes())
                || plainly_non_numeric(right.as_bytes()))
        {
            return Ok(left == right);
        }
        let compare = match self {
            Self::Strict => strict_equals,
            Self::Loose => loose_equals,
        };
        Ok(matches!(
            compare(left, right, context.diagnostics())?,
            Value::Bool(true)
        ))
    }
}

/// `in_array(mixed $needle, array $haystack, bool $strict = false): bool`
///
/// ```php
/// in_array("1e1", [10]);       // true: "1e1" == 10
/// in_array("1e1", [10], true); // false
/// ```
pub(super) const IN_ARRAY: FunctionEntry = FunctionEntry::asynchronous(
    "in_array",
    &[
        Param::new("needle", ParamType::Mixed),
        Param::new("haystack", ParamType::Array),
        Param::new("strict", ParamType::Bool).optional(),
    ],
    in_array,
);

fn in_array(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let equality = Equality::from_strict_argument(&arguments);
        let needle = &arguments[0];
        for value in required_array(&arguments, 1).values() {
            if equality.equal(context, needle, value).await? {
                return Ok(Value::Bool(true));
            }
        }
        Ok(Value::Bool(false))
    })
}

/// `array_search(mixed $needle, array $haystack, bool $strict = false):
/// int|string|false`, the first key whose value equals the needle.
///
/// ```php
/// array_search("b", ["x" => "a", "y" => "b"]); // "y"
/// array_search("c", ["a"]);                    // false
/// ```
pub(super) const ARRAY_SEARCH: FunctionEntry = FunctionEntry::asynchronous(
    "array_search",
    &[
        Param::new("needle", ParamType::Mixed),
        Param::new("haystack", ParamType::Array),
        Param::new("strict", ParamType::Bool).optional(),
    ],
    array_search,
);

fn array_search(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let equality = Equality::from_strict_argument(&arguments);
        let needle = &arguments[0];
        for (key, value) in required_array(&arguments, 1).iter() {
            if equality.equal(context, needle, value).await? {
                return Ok(Value::from(key));
            }
        }
        Ok(Value::Bool(false))
    })
}
