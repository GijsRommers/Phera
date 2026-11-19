//! `[new Label] == ['label']` can pause at an element that needs __toString.
//! The VM runs the method. The comparison retains its position on the heap.

use std::rc::Rc;

use crate::{Diagnostics, Object, OpError, PhpString, Value};

use super::structure::{CompareStep, Comparison, Pair, Table, TableComparison};
use super::{ObjectComparator, compare_resolved, smart_strcmp};

pub enum StringComparisonStep {
    Complete(i8),
    Cast(Box<StringComparison>),
    Initialize(Box<LazyComparison>),
}

pub struct StringComparison {
    pair: CastPair,
    remaining: Option<TableComparison>,
}

/// The exact object operands and parent table cursor survive initialization.
pub struct LazyComparison {
    left: Rc<Object>,
    right: Rc<Object>,
    remaining: Option<TableComparison>,
}

impl LazyComparison {
    pub fn pending(&self) -> Option<Rc<Object>> {
        [&self.left, &self.right].into_iter().find_map(|original| {
            let mut object = original.clone();
            while let Some(instance) = object.lazy_instance() {
                object = instance;
            }
            object.is_uninitialized_lazy().then_some(object)
        })
    }

    pub fn resume(
        mut self,
        objects: &mut ObjectComparator<'_>,
        diagnostics: &mut dyn Diagnostics,
    ) -> Result<StringComparisonStep, OpError> {
        let mut comparison = Comparison::Native(objects);
        match comparison.objects(&self.left, &self.right, diagnostics)? {
            Pair::Compared(order) if order != 0 => Ok(StringComparisonStep::Complete(order)),
            Pair::Compared(_) => match self.remaining {
                Some(table) => resume_table(table, &mut comparison, diagnostics),
                None => Ok(StringComparisonStep::Complete(0)),
            },
            Pair::Initialize => Ok(StringComparisonStep::Initialize(Box::new(self))),
            Pair::Tables(left, right) => {
                let table = match self.remaining.take() {
                    Some(mut table) => {
                        table.enter_child(left, right, &comparison)?;
                        table
                    }
                    None => TableComparison::new(left, right, &comparison)?,
                };
                resume_table(table, &mut comparison, diagnostics)
            }
        }
    }
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
                Pair::Initialize => {
                    return Ok(StringComparisonStep::Initialize(Box::new(LazyComparison {
                        left: left.clone(),
                        right: right.clone(),
                        remaining: None,
                    })));
                }
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
        CompareStep::Initialize(left, right) => {
            Ok(StringComparisonStep::Initialize(Box::new(LazyComparison {
                left,
                right,
                remaining: Some(table),
            })))
        }
        CompareStep::Cast(pair) => Ok(StringComparisonStep::Cast(Box::new(StringComparison {
            pair,
            remaining: Some(table),
        }))),
    }
}
