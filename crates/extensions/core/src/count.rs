//! `count` and its alias `sizeof`.

use std::collections::HashSet;
use std::rc::Rc;

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

pub(crate) const COUNT_NORMAL: i64 = 0;
pub(crate) const COUNT_RECURSIVE: i64 = 1;

pub(crate) fn register(registry: &mut Registry) {
    // Keep PHP's union diagnostics and validate the mode before calling count().
    const PARAMS: &[Param] = &[
        Param::new("value", ParamType::Mixed),
        Param::new("mode", ParamType::Int).optional(),
    ];
    registry.function(FunctionEntry::asynchronous("count", PARAMS, count));
    registry.function(FunctionEntry::asynchronous("sizeof", PARAMS, sizeof));
    registry.constant("COUNT_NORMAL", COUNT_NORMAL);
    registry.constant("COUNT_RECURSIVE", COUNT_RECURSIVE);
}

fn count(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(count_as("count", context, arguments))
}

fn sizeof(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(count_as("sizeof", context, arguments))
}

/// `name` is the name the function was called by, which PHP uses in errors.
async fn count_as(name: &str, context: &mut dyn Context, arguments: Vec<Value>) -> NativeResult {
    let mode = args::int(&arguments, 1).unwrap_or(COUNT_NORMAL);
    if mode != COUNT_NORMAL && mode != COUNT_RECURSIVE {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #2 ($mode) must be either COUNT_NORMAL or COUNT_RECURSIVE"
        )));
    }
    let value = args::value(&arguments, 0);
    if let Value::Object(object) = value
        && object.class.is_countable()
    {
        return context.count_object(object.clone()).await;
    }
    let Value::Array(array) = value else {
        let given = match value {
            Value::Bool(true) => "true".to_string(),
            Value::Bool(false) => "false".to_string(),
            other => other.type_name(),
        };
        return Err(NativeError::type_error(format!(
            "{name}(): Argument #1 ($value) must be of type Countable|array, {given} given"
        )));
    };
    Ok(Value::Int(if mode == COUNT_RECURSIVE {
        count_recursive(array, context)
    } else {
        array.len() as i64
    }))
}

/// Counts the elements of an array and of every array inside it. An explicit
/// stack instead of recursion, so nesting depth is limited by memory only.
fn count_recursive(array: &Rc<PhpArray>, context: &mut dyn Context) -> i64 {
    let mut walk = Walk::default();
    walk.enter(array, context);
    while let Some((array, next)) = walk.pending.last_mut() {
        let Some(position) = *next else {
            // Counted in full: meeting it again elsewhere is not recursion.
            walk.active.remove(&identity(array));
            walk.pending.pop();
            continue;
        };
        *next = array.next_position(position);
        let inner = array
            .entry_at(position)
            .and_then(|(_, item)| match &*item.deref() {
                Value::Array(inner) => Some(inner.clone()),
                _ => None,
            });
        if let Some(inner) = inner {
            walk.enter(&inner, context);
        }
    }
    walk.total
}

fn identity(array: &Rc<PhpArray>) -> usize {
    Rc::as_ptr(array) as usize
}

#[derive(Default)]
struct Walk {
    total: i64,
    /// Arrays whose elements are being counted, with the next position in each.
    pending: Vec<(Rc<PhpArray>, Option<usize>)>,
    active: HashSet<usize>,
}

impl Walk {
    fn enter(&mut self, array: &Rc<PhpArray>, context: &mut dyn Context) {
        // An array reached again while it is still being counted contains
        // itself through a reference.
        if !self.active.insert(identity(array)) {
            context.warning("count(): Recursion detected".to_string());
            return;
        }
        self.total = self.total.saturating_add(array.len() as i64);
        self.pending.push((array.clone(), array.first_position()));
    }
}
