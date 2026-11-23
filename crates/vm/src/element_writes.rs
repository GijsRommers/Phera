//! A plain write of an array element, at the end of a few array keys.
//!
//! ```php
//! $row['id'] = $id;                // one key
//! $map[$group][$name] = $value;    // two keys, both levels arrays already
//! $lists[$group][] = $value;       // appended to an array that exists
//! ```
//!
//! This is the whole of such a write when every level but the last exists
//! as an array and the last element is not a PHP reference: each level is
//! separated from other owners (copy on write) and the last key gets the
//! value (or, for `[]`, the next free integer key). [`writable`] checks
//! that before anything changes, so a caller that is refused leaves the
//! place as it was for the general path, which handles creating levels,
//! references, `ArrayAccess` and the warnings.

use std::rc::Rc;

use phera_value::{ArrayKey, PhpArray, Value};

/// How many keys a plain write walks.
pub(crate) const MOST_KEYS: usize = 3;

/// Where a plain write stores its value: the element at the last key, or
/// for `$a[...][] = $value` the next free integer key of the array the
/// keys lead to.
#[derive(Clone, Copy)]
pub(crate) enum Target {
    Last,
    Append,
}

/// The arrays a write walks through, and the key it stores at: all of
/// `keys` are levels when it appends.
fn split(keys: &[ArrayKey], target: Target) -> Option<(&[ArrayKey], Option<&ArrayKey>)> {
    match target {
        Target::Last => {
            let (last, levels) = keys.split_last()?;
            Some((levels, Some(last)))
        }
        Target::Append => Some((keys, None)),
    }
}

/// Whether writing at `keys` of `array` is a plain write.
pub(crate) fn writable(array: &PhpArray, keys: &[ArrayKey], target: Target) -> bool {
    let Some((levels, last)) = split(keys, target) else {
        return false;
    };
    let mut current = array;
    for key in levels {
        current = match current.get(key) {
            Some(Value::Array(inner)) => inner,
            _ => return false,
        };
    }
    last.is_none_or(|last| !matches!(current.get(last), Some(Value::Reference(_))))
}

/// Stores `value` at `keys`, once [`writable`] has said yes. Gives what
/// the element held, for the caller to release; `None` if a level was not
/// an array after all, or an append found no free integer key (the VM
/// warns), with nothing written.
pub(crate) fn write(
    array: &mut Rc<PhpArray>,
    keys: &[ArrayKey],
    target: Target,
    value: Value,
) -> Option<Option<Value>> {
    let (levels, last) = split(keys, target)?;
    let mut current = Rc::make_mut(array);
    for key in levels {
        current = match current.get_mut(key) {
            Some(Value::Array(inner)) => Rc::make_mut(inner),
            _ => return None,
        };
    }
    match last {
        Some(last) => Some(current.insert(last.clone(), value)),
        None => current.push(value).ok().map(|_| None),
    }
}
