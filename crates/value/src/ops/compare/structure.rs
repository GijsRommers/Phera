//! Walk array entries and object properties in PHP's comparison order.
//!
//! ```php
//! [1, 'x' => 2] == ['x' => 2, 1];  // true
//! [1, 'x' => 2] === ['x' => 2, 1]; // false
//! ```

use std::cell::Ref;
use std::collections::HashSet;
use std::ops::Deref;
use std::rc::Rc;

use crate::array::PhpArray;
use crate::error::{Diagnostics, OpError};
use crate::object::Object;
use crate::value::Value;

use super::casting::CastPair;
use super::{ObjectEquality, cmp3, compare_leaf, identical_leaf};

#[derive(Clone, Copy)]
pub(super) enum Comparison {
    Strict,
    Loose,
    PendingError,
    NativeEquality(ObjectEquality),
}

impl Comparison {
    /// An object pair can compare as a leaf, even inside an array.
    pub(super) fn object_equality(self, left: &Rc<Object>, right: &Rc<Object>) -> Option<i8> {
        if Rc::ptr_eq(left, right) {
            return Some(0);
        }
        if left.class.name() != right.class.name() || matches!(self, Self::Strict) {
            return Some(1);
        }
        match self {
            Self::NativeEquality(compare) => compare(left, right).map(|equal| i8::from(!equal)),
            _ => None,
        }
    }

    pub(super) fn leaf(
        self,
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
    Object(Rc<Object>),
}

enum Entries<'a> {
    Array(&'a PhpArray),
    Object(Ref<'a, PhpArray>),
}

impl Deref for Entries<'_> {
    type Target = PhpArray;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Array(entries) => entries,
            Self::Object(entries) => entries,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum TableId {
    Array(*const PhpArray),
    Object(*const Object),
}

impl Table {
    pub(super) fn object(object: &Rc<Object>) -> Self {
        Self::Object(object.clone())
    }

    /// A cast may change a subsequently compared property. Borrow only while
    /// advancing, so the next step observes the current table after PHP runs.
    fn entries(&self) -> Result<Entries<'_>, OpError> {
        match self {
            Self::Array(array) => Ok(Entries::Array(array)),
            Self::Object(object) => {
                object
                    .properties
                    .try_borrow()
                    .map(Entries::Object)
                    .map_err(|_| {
                        OpError::error(
                            "Cannot compare an object while its properties are being modified",
                        )
                    })
            }
        }
    }

    fn id(&self) -> TableId {
        match self {
            Self::Array(array) => TableId::Array(Rc::as_ptr(array)),
            Self::Object(object) => TableId::Object(Rc::as_ptr(object)),
        }
    }

    /// PHP checks the native stack before comparing each table. Charge the
    /// frame sizes observed in the default PHP 8.5 arm64 build against a
    /// fixed budget, so long acyclic chains get a separate catchable error.
    fn stack_cost(&self, comparison: Comparison) -> usize {
        match self {
            Self::Array(_) if matches!(comparison, Comparison::Strict) => 128,
            Self::Array(_) => 256,
            Self::Object(_) => 352,
        }
    }

    fn stack_error(&self) -> OpError {
        OpError::error(match self {
            Self::Array(_) => "Maximum call stack size reached during comparison",
            Self::Object(_) => "Maximum call stack size reached during object comparison",
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
    fn new(left: Table, right: Table, comparison: Comparison) -> Result<Self, OpError> {
        let left_position = left.entries()?.first_position();
        let right_position = match comparison {
            Comparison::Strict => right.entries()?.first_position(),
            _ => None,
        };
        Ok(Self {
            left,
            right,
            left_position,
            right_position,
        })
    }
}

pub(super) enum CompareStep {
    Complete(i8),
    Cast(CastPair),
}

/// PHP protects only the active left table. Parents and the active set
/// stay empty for flat operands, and remain owned if a leaf must run PHP.
pub(super) struct TableComparison {
    frame: Frame,
    parents: Vec<Frame>,
    active: Option<HashSet<TableId>>,
    budget: usize,
    comparison: Comparison,
}

impl TableComparison {
    pub(super) fn new(left: Table, right: Table, comparison: Comparison) -> Result<Self, OpError> {
        let stack_budget = match comparison {
            Comparison::Strict => 8_329_216,
            _ => 8_329_215,
        };
        Ok(Self {
            budget: stack_budget - left.stack_cost(comparison),
            parents: Vec::new(),
            active: None,
            frame: Frame::new(left, right, comparison)?,
            comparison,
        })
    }

    pub(super) fn advance<const CAST_STRINGS: bool>(
        &mut self,
        diagnostics: &mut dyn Diagnostics,
    ) -> Result<CompareStep, OpError> {
        let Self {
            frame,
            parents,
            active,
            budget,
            comparison,
        } = self;
        let comparison = *comparison;
        loop {
            let left = frame.left.entries()?;
            let right = frame.right.entries()?;
            if left.len() != right.len() {
                return Ok(CompareStep::Complete(cmp3(left.len().cmp(&right.len()))));
            }
            let mut child = None;
            while let Some(position) = frame.left_position {
                let Some((key, left_value)) = left.entry_at(position) else {
                    return Ok(CompareStep::Complete(1));
                };
                frame.left_position = left.next_position(position);
                let right_value = match comparison {
                    Comparison::Strict => {
                        let Some(position) = frame.right_position else {
                            return Ok(CompareStep::Complete(1));
                        };
                        let Some((right_key, value)) = right.entry_at(position) else {
                            return Ok(CompareStep::Complete(1));
                        };
                        if key != right_key {
                            return Ok(CompareStep::Complete(1));
                        }
                        frame.right_position = right.next_position(position);
                        value
                    }
                    _ => {
                        let Some(value) = right.get(&key) else {
                            return Ok(CompareStep::Complete(1));
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
                        if let Some(result) = comparison.object_equality(a, b) {
                            result
                        } else {
                            child = Some((Table::object(a), Table::object(b)));
                            break;
                        }
                    }
                    _ => {
                        if CAST_STRINGS
                            && let Some(pair) = CastPair::from_values(&left_value, &right_value)
                        {
                            return Ok(CompareStep::Cast(pair));
                        }
                        comparison.leaf(&left_value, &right_value, diagnostics)?
                    }
                };
                if result != 0 {
                    return Ok(CompareStep::Complete(result));
                }
            }

            drop(left);
            drop(right);
            if let Some((left, right)) = child {
                *budget = budget
                    .checked_sub(left.stack_cost(comparison))
                    .ok_or_else(|| left.stack_error())?;
                let active = active.get_or_insert_with(HashSet::new);
                active.insert(frame.left.id());
                if active.contains(&left.id()) {
                    return Err(OpError::error(
                        "Nesting level too deep - recursive dependency?",
                    ));
                }
                let parent = std::mem::replace(frame, Frame::new(left, right, comparison)?);
                parents.push(parent);
            } else if let Some(parent) = parents.pop() {
                if let Some(active) = active {
                    active.remove(&parent.left.id());
                }
                *budget += frame.left.stack_cost(comparison);
                *frame = parent;
            } else {
                return Ok(CompareStep::Complete(0));
            }
        }
    }
}

#[inline(never)]
pub(super) fn compare_tables(
    left: Table,
    right: Table,
    comparison: Comparison,
    diagnostics: &mut dyn Diagnostics,
) -> Result<i8, OpError> {
    match TableComparison::new(left, right, comparison)?.advance::<false>(diagnostics)? {
        CompareStep::Complete(order) => Ok(order),
        CompareStep::Cast(_) => Err(OpError::error(
            "Unexpected PHP string conversion during value comparison",
        )),
    }
}
