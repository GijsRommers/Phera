//! Callback sets preserve first-array keys: `array_udiff([3, 2], [2], 'strcmp')`.

use std::cmp::Ordering;
use std::rc::Rc;

use phera_extension_api::{
    AsyncFn, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    PhpArray, Registry, Value,
};

use super::callback::Comparison;

pub(super) type Entry = (phera_extension_api::ArrayKey, Value);

const PARAMS: &[Param] = &[
    // PHP parses trailing callbacks before checking array types and requires
    // a different minimum arity for one versus two callbacks. The generic API
    // cannot express that order, so run() performs the complete validation.
    Param::new("array", ParamType::Mixed).optional(),
    Param::new("rest", ParamType::Mixed).variadic(),
];

pub(crate) fn register(registry: &mut Registry) {
    for (name, handler) in [
        ("array_udiff", udiff as AsyncFn),
        ("array_udiff_assoc", udiff_assoc),
        ("array_udiff_uassoc", udiff_uassoc),
        ("array_diff_ukey", diff_ukey),
        ("array_diff_uassoc", diff_uassoc),
        ("array_uintersect", uintersect),
        ("array_uintersect_assoc", uintersect_assoc),
        ("array_uintersect_uassoc", uintersect_uassoc),
        ("array_intersect_ukey", intersect_ukey),
        ("array_intersect_uassoc", intersect_uassoc),
    ] {
        registry.function(FunctionEntry::asynchronous(name, PARAMS, handler));
    }
}

fn udiff(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, args, "array_udiff"))
}
fn udiff_assoc(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, args, "array_udiff_assoc"))
}
fn udiff_uassoc(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, args, "array_udiff_uassoc"))
}
fn diff_ukey(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, args, "array_diff_ukey"))
}
fn diff_uassoc(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, args, "array_diff_uassoc"))
}
fn uintersect(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, args, "array_uintersect"))
}
fn uintersect_assoc(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, args, "array_uintersect_assoc"))
}
fn uintersect_uassoc(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, args, "array_uintersect_uassoc"))
}
fn intersect_ukey(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, args, "array_intersect_ukey"))
}
fn intersect_uassoc(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, args, "array_intersect_uassoc"))
}

pub(super) struct SetComparison<'a> {
    pub comparison: Comparison<'a>,
    pub data: Option<&'a Value>,
    pub keys: Option<&'a Value>,
    pub key_only: bool,
}

impl SetComparison<'_> {
    pub async fn order(
        &mut self,
        context: &mut dyn Context,
        left: &Entry,
        right: &Entry,
    ) -> Result<Ordering, NativeError> {
        if let Some(callback) = self.keys {
            self.comparison
                .compare(
                    context,
                    callback,
                    super::key_value(&left.0),
                    super::key_value(&right.0),
                    true,
                )
                .await
        } else {
            self.data(context, left, right, true).await
        }
    }

    /// php_array_intersect leaves its global callback set to the data callback
    /// after a successful associative value comparison. Later key scans in the
    /// same outer iteration therefore use that callback on keys too.
    pub async fn continuing_order(
        &mut self,
        context: &mut dyn Context,
        left: &Entry,
        right: &Entry,
        data_compares_keys: bool,
    ) -> Result<Ordering, NativeError> {
        if data_compares_keys && let Some(callback) = self.data {
            return self
                .comparison
                .compare(
                    context,
                    callback,
                    super::key_value(&left.0),
                    super::key_value(&right.0),
                    true,
                )
                .await;
        }
        self.order(context, left, right).await
    }

    pub async fn data(
        &mut self,
        context: &mut dyn Context,
        left: &Entry,
        right: &Entry,
        boolean_retry: bool,
    ) -> Result<Ordering, NativeError> {
        if let Some(callback) = self.data {
            self.comparison
                .compare(
                    context,
                    callback,
                    left.1.clone(),
                    right.1.clone(),
                    boolean_retry,
                )
                .await
        } else {
            super::ordering::compare_values(
                context,
                &left.1,
                &right.1,
                super::ordering::SortOrder::String { ignore_case: false },
            )
        }
    }
}

async fn run(context: &mut dyn Context, arguments: Vec<Value>, name: &str) -> NativeResult {
    let user_data = name.starts_with("array_u");
    let user_keys = name.ends_with("ukey") || name.ends_with("uassoc");
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
        super::callback::validate(name, index + 1, false, callback, false)?;
    }
    let mut comparison = SetComparison {
        comparison: Comparison {
            name,
            deprecated: false,
        },
        data: user_data.then(|| &arguments[array_count]),
        keys: user_keys.then(|| &arguments[arguments.len() - 1]),
        key_only: name.ends_with("ukey"),
    };
    let exact_keys = name.ends_with("_assoc");
    let intersection = name.contains("intersect");
    let mut arrays = Vec::new();
    let mut sorted = Vec::new();
    for (index, value) in arguments.iter().take(array_count).enumerate() {
        let Value::Array(array) = value else {
            let parameter = if index == 0 { " ($array)" } else { "" };
            return Err(NativeError::type_error(format!(
                "{name}(): Argument #{}{parameter} must be of type array, {} given",
                index + 1,
                crate::argument_type_name(value)
            )));
        };
        arrays.push(array.clone());
        if !exact_keys {
            let entries: Vec<Entry> = array
                .iter()
                .map(|(key, value)| (key, value.clone()))
                .collect();
            let indices = super::sort_order_async::order(entries.len(), async |left, right| {
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
        super::callback_set_merge::run(context, &arrays[0], &sorted, &mut comparison, intersection)
            .await
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
                        false,
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
