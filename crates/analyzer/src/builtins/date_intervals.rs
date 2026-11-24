//! `$start->diff($end)->days` is an integer. A DateInterval created from a
//! duration string can instead have a false day count.

use crate::{
    index::Index,
    types::{Field, Shape, ShapeKey, ShapeKind, ValueType as T},
};
use std::sync::Arc;

pub(super) fn refine(index: &mut Index) {
    let result = T::intersection(
        &T::Class("DateInterval".into()),
        &T::Shape(Arc::new(Shape {
            fields: vec![Field {
                key: ShapeKey::String("days".into()),
                value: T::IntRange(0, i64::MAX),
                optional: false,
            }],
            remainder: None,
            kind: ShapeKind::Object,
        })),
    );
    for name in ["datetime", "datetimeimmutable", "datetimeinterface"] {
        if let Some(signature) = index
            .classes
            .get_mut(name)
            .and_then(|class| class.methods.get_mut("diff"))
        {
            signature.returns = result.clone();
        }
    }
    if let Some(signature) = index.functions.get_mut("date_diff") {
        signature.returns = result;
    }
}
