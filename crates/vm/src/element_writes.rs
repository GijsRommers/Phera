//! A plain write of an array element, at the end of a few array keys.
//!
//! ```php
//! $row['id'] = $id;                // one key
//! $map[$group][$name] = $value;    // two keys, both levels arrays already
//! ```
//!
//! This is the whole of such a write when every level but the last exists
//! as an array and the last element is not a PHP reference: each level is
//! separated from other owners (copy on write) and the last key gets the
//! value. [`writable`] checks that before anything changes, so a caller
//! that is refused leaves the place as it was for the general path, which
//! handles creating levels, references, `ArrayAccess` and the warnings.

use std::rc::Rc;

use phera_value::{ArrayKey, PhpArray, Value};

/// How many keys a plain write walks.
pub(crate) const MOST_KEYS: usize = 3;

/// Whether writing at `keys` of `array` is a plain write.
pub(crate) fn writable(array: &PhpArray, keys: &[ArrayKey]) -> bool {
    let Some((last, levels)) = keys.split_last() else {
        return false;
    };
    let mut current = array;
    for key in levels {
        current = match current.get(key) {
            Some(Value::Array(inner)) => inner,
            _ => return false,
        };
    }
    !matches!(current.get(last), Some(Value::Reference(_)))
}

/// Stores `value` at `keys`, once [`writable`] has said yes. Gives what
/// the element held, for the caller to release; `None` if a level was not
/// an array after all, with nothing written.
pub(crate) fn write(
    array: &mut Rc<PhpArray>,
    keys: &[ArrayKey],
    value: Value,
) -> Option<Option<Value>> {
    let (last, levels) = keys.split_last()?;
    let mut current = Rc::make_mut(array);
    for key in levels {
        current = match current.get_mut(key) {
            Some(Value::Array(inner)) => Rc::make_mut(inner),
            _ => return None,
        };
    }
    Some(current.insert(last.clone(), value))
}
