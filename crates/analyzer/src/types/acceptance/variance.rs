//! A covariant `Source<Dog>` can satisfy `Source<Animal>`.
//! Invariant arguments must match in both directions.

use crate::{index::Index, phpdoc::Variance, types::*};

pub(super) fn requirements(
    expected: &Generic,
    given: &ValueType,
    index: &Index,
) -> Option<Vec<(Variance, ValueType, ValueType)>> {
    let docs = &index.class(&expected.base)?.documented;
    if docs.template_names.len() != expected.arguments.len() || expected.arguments.is_empty() {
        return None;
    }
    let projected;
    let actual = if let ValueType::Generic(actual) = given
        && actual.base.eq_ignore_ascii_case(&expected.base)
    {
        &actual.arguments
    } else {
        projected = crate::signature_types::project_generic(index, &expected.base, given)?;
        &projected
    };
    if expected.arguments.len() != actual.len() {
        return None;
    }
    let mut comparisons = Vec::with_capacity(actual.len());
    for ((name, expected), actual) in docs
        .template_names
        .iter()
        .zip(&expected.arguments)
        .zip(actual)
    {
        // An unbound argument is a constraint on a future choice and does
        // not prove that two concrete types are incompatible.
        if expected.is_mixed()
            || actual.is_mixed()
            || matches!(expected, ValueType::Template(_))
            || matches!(actual, ValueType::Template(_))
        {
            return None;
        }
        let variance = docs
            .template_variances
            .get(name)
            .copied()
            .unwrap_or_default();
        comparisons.push((variance, expected.clone(), actual.clone()));
    }
    Some(comparisons)
}
