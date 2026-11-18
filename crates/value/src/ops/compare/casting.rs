//! `[new Label] == ['label']` can pause at an element that needs __toString.
//! The VM runs the method. The comparison retains its position on the heap.

use std::rc::Rc;

use crate::{Diagnostics, Object, OpError, PhpString, Value};

use super::structure::{CompareStep, Comparison, Pair, Table, TableComparison};
use super::{ObjectComparator, compare_resolved, smart_strcmp};

pub enum StringComparisonStep {
    Complete(i8),
    Cast(Box<StringComparison>),
}

pub struct StringComparison {
    pair: CastPair,
    remaining: Option<TableComparison>,
}

pub(super) struct CastPair {
    object: Rc<Object>,
    other: PhpString,
    side: Side,
}

enum Side {
    Left,
    Right,
}

impl CastPair {
    pub(super) fn from_values(left: &Value, right: &Value) -> Option<Self> {
        let (object, other, side) = match (left, right) {
            (Value::Object(object), Value::String(string)) => (object, string, Side::Left),
            (Value::String(string), Value::Object(object)) => (object, string, Side::Right),
            _ => return None,
        };
        object.class.is_instance_of("Stringable").then(|| Self {
            object: object.clone(),
            other: other.clone(),
            side,
        })
    }
}

impl StringComparison {
    pub fn object(&self) -> &Rc<Object> {
        &self.pair.object
    }

    pub fn resume(
        self,
        string: Option<PhpString>,
        objects: &mut ObjectComparator<'_>,
        diagnostics: &mut dyn Diagnostics,
    ) -> Result<StringComparisonStep, OpError> {
        let Self { pair, remaining } = self;
        let order = match (pair.side, string) {
            (Side::Left, Some(string)) => smart_strcmp(string.as_bytes(), pair.other.as_bytes()),
            (Side::Right, Some(string)) => smart_strcmp(pair.other.as_bytes(), string.as_bytes()),
            (Side::Left, None) => 1,
            (Side::Right, None) => -1,
        };
        match remaining {
            Some(table) if order == 0 => {
                resume_table(table, &mut Comparison::Native(objects), diagnostics)
            }
            _ => Ok(StringComparisonStep::Complete(order)),
        }
    }
}

/// Compare in PHP's array and property order, yielding the first object that
/// has to run code. Values without a stringable comparison finish here.
pub fn compare_with_string_objects(
    left: &Value,
    right: &Value,
    objects: &mut ObjectComparator<'_>,
    diagnostics: &mut dyn Diagnostics,
) -> Result<StringComparisonStep, OpError> {
    let left = left.try_deref()?;
    let right = right.try_deref()?;
    if let Some(pair) = CastPair::from_values(&left, &right) {
        return Ok(StringComparisonStep::Cast(Box::new(StringComparison {
            pair,
            remaining: None,
        })));
    }
    let mut comparison = Comparison::Native(objects);
    let (left, right) = match (&*left, &*right) {
        (Value::Array(left), Value::Array(right)) => {
            if Rc::ptr_eq(left, right) {
                return Ok(StringComparisonStep::Complete(0));
            }
            (Table::Array(left.clone()), Table::Array(right.clone()))
        }
        (Value::Object(left), Value::Object(right)) => {
            match comparison.objects(left, right, diagnostics)? {
                Pair::Compared(result) => return Ok(StringComparisonStep::Complete(result)),
                Pair::Tables(left, right) => (left, right),
            }
        }
        _ => {
            return compare_resolved(&left, &right, &mut comparison, diagnostics)
                .map(StringComparisonStep::Complete);
        }
    };
    resume_table(
        TableComparison::new(left, right, &comparison)?,
        &mut comparison,
        diagnostics,
    )
}

fn resume_table(
    mut table: TableComparison,
    comparison: &mut Comparison<'_, '_>,
    diagnostics: &mut dyn Diagnostics,
) -> Result<StringComparisonStep, OpError> {
    match table.advance::<true>(comparison, diagnostics)? {
        CompareStep::Complete(order) => Ok(StringComparisonStep::Complete(order)),
        CompareStep::Cast(pair) => Ok(StringComparisonStep::Cast(Box::new(StringComparison {
            pair,
            remaining: Some(table),
        }))),
    }
}
