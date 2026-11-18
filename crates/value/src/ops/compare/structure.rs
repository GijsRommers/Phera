//! Walk array entries and object properties in PHP's comparison order.
//!
//! ```php
//! [1, 'x' => 2] == ['x' => 2, 1];  // true
//! [1, 'x' => 2] === ['x' => 2, 1]; // false
//! ```

use std::collections::HashSet;
use std::rc::Rc;

use crate::array::PhpArray;
use crate::error::{Diagnostics, OpError};
use crate::object::Object;
use crate::value::Value;

use super::{ObjectComparator, ObjectComparison, cmp3, compare_leaf, identical_leaf};

pub(super) enum Comparison<'a, 'callback> {
    Strict,
    Loose,
    PendingError,
    Native(&'a mut ObjectComparator<'callback>),
}

pub(super) enum Pair {
    Compared(i8),
    Tables(Table, Table),
}

impl Comparison<'_, '_> {
    /// Native handlers can also compare objects that have different class names.
    pub(super) fn objects(
        &mut self,
        left: &Rc<Object>,
        right: &Rc<Object>,
        diagnostics: &mut dyn Diagnostics,
    ) -> Result<Pair, OpError> {
        if Rc::ptr_eq(left, right) {
            return Ok(Pair::Compared(0));
        }
        if matches!(self, Self::Strict) {
            return Ok(Pair::Compared(1));
        }
        if let Self::Native(compare) = self {
            match compare(left, right, diagnostics)? {
                ObjectComparison::Compared(result) => return Ok(Pair::Compared(result)),
                ObjectComparison::Projected(a, b) => {
                    return Ok(Pair::Tables(
                        Table::Object(left.clone(), a),
                        Table::Object(right.clone(), b),
                    ));
                }
                ObjectComparison::Default => {}
            }
        }
        if left.class.name() != right.class.name() {
            return Ok(Pair::Compared(1));
        }
        Ok(Pair::Tables(Table::object(left)?, Table::object(right)?))
    }

    pub(super) fn leaf(
        &self,
        left: &Value,
        right: &Value,
        diagnostics: &mut dyn Diagnostics,
    ) -> Result<i8, OpError> {
        match self {
            Self::Strict => Ok(i8::from(!identical_leaf(left, right))),
            _ => compare_leaf(left, right, self, diagnostics),
        }
    }
}

pub(super) enum Table {
    Array(Rc<PhpArray>),
    Object(Rc<Object>, PhpArray),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum TableId {
    Array(*const PhpArray),
    Object(*const Object),
}

impl Table {
    pub(super) fn object(object: &Rc<Object>) -> Result<Self, OpError> {
        let properties = object.properties.try_borrow().map_err(|_| {
            OpError::error("Cannot compare an object while its properties are being modified")
        })?;
        Ok(Self::Object(object.clone(), properties.clone()))
    }

    fn entries(&self) -> &PhpArray {
        match self {
            Self::Array(array) => array,
            Self::Object(_, properties) => properties,
        }
    }

    fn id(&self) -> TableId {
        match self {
            Self::Array(array) => TableId::Array(Rc::as_ptr(array)),
            Self::Object(object, _) => TableId::Object(Rc::as_ptr(object)),
        }
    }

    /// PHP checks the native stack before comparing each table. Charge the
    /// frame sizes observed in the default PHP 8.5 arm64 build against a
    /// fixed budget, so long acyclic chains get a separate catchable error.
    fn stack_cost(&self, comparison: &Comparison<'_, '_>) -> usize {
        match self {
            Self::Array(_) if matches!(comparison, Comparison::Strict) => 128,
            Self::Array(_) => 256,
            Self::Object(_, _) => 352,
        }
    }

    fn stack_error(&self) -> OpError {
        OpError::error(match self {
            Self::Array(_) => "Maximum call stack size reached during comparison",
            Self::Object(_, _) => "Maximum call stack size reached during object comparison",
        })
    }
}

/// Each frame owns the tables and remembers the positions after the last pair.
struct Frame {
    left: Table,
    right: Table,
    left_position: Option<usize>,
    right_position: Option<usize>,
}

impl Frame {
    fn new(left: Table, right: Table, comparison: &Comparison<'_, '_>) -> Self {
        let left_position = left.entries().first_position();
        let right_position = match comparison {
            Comparison::Strict => right.entries().first_position(),
            _ => None,
        };
        Self {
            left,
            right,
            left_position,
            right_position,
        }
    }
}

/// PHP protects only the active left table. A table reused by a sibling or
/// compared with itself is safe. Parents and the active set stay empty
/// while comparing flat arrays.
#[inline(never)]
pub(super) fn compare_tables(
    left: Table,
    right: Table,
    comparison: &mut Comparison<'_, '_>,
    diagnostics: &mut dyn Diagnostics,
) -> Result<i8, OpError> {
    let stack_budget = match comparison {
        Comparison::Strict => 8_329_216,
        _ => 8_329_215,
    };
    let mut budget = stack_budget - left.stack_cost(comparison);
    let mut parents = Vec::new();
    let mut active = None;
    let mut frame = Frame::new(left, right, comparison);
    loop {
        let left = frame.left.entries();
        let right = frame.right.entries();
        if left.len() != right.len() {
            return Ok(cmp3(left.len().cmp(&right.len())));
        }
        let mut child = None;
        while let Some(position) = frame.left_position {
            let Some((key, left_value)) = left.entry_at(position) else {
                return Ok(1);
            };
            frame.left_position = left.next_position(position);
            let right_value = match comparison {
                Comparison::Strict => {
                    let Some(position) = frame.right_position else {
                        return Ok(1);
                    };
                    let Some((right_key, value)) = right.entry_at(position) else {
                        return Ok(1);
                    };
                    if key != right_key {
                        return Ok(1);
                    }
                    frame.right_position = right.next_position(position);
                    value
                }
                _ => {
                    let Some(value) = right.get(&key) else {
                        return Ok(1);
                    };
                    value
                }
            };

            let left_value = left_value.try_deref()?;
            let right_value = right_value.try_deref()?;
            let result = match (&*left_value, &*right_value) {
                (Value::Array(a), Value::Array(b)) => {
                    if Rc::ptr_eq(a, b) {
                        continue;
                    }
                    child = Some((Table::Array(a.clone()), Table::Array(b.clone())));
                    break;
                }
                (Value::Object(a), Value::Object(b)) => {
                    match comparison.objects(a, b, diagnostics)? {
                        Pair::Compared(result) => result,
                        Pair::Tables(left, right) => {
                            child = Some((left, right));
                            break;
                        }
                    }
                }
                _ => comparison.leaf(&left_value, &right_value, diagnostics)?,
            };
            if result != 0 {
                return Ok(result);
            }
        }

        if let Some((left, right)) = child {
            budget = budget
                .checked_sub(left.stack_cost(comparison))
                .ok_or_else(|| left.stack_error())?;
            let active = active.get_or_insert_with(HashSet::new);
            active.insert(frame.left.id());
            if active.contains(&left.id()) {
                return Err(OpError::error(
                    "Nesting level too deep - recursive dependency?",
                ));
            }
            parents.push(frame);
            frame = Frame::new(left, right, comparison);
        } else if let Some(parent) = parents.pop() {
            if let Some(active) = &mut active {
                active.remove(&parent.left.id());
            }
            budget += frame.left.stack_cost(comparison);
            frame = parent;
        } else {
            return Ok(0);
        }
    }
}
