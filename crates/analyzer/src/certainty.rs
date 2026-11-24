//! `/** @param string $x */ function f($x) { return is_string($x); }` can be
//! constant only if the PHPDoc is considered certain at runtime.

use crate::diagnostic::Diagnostic;
use std::collections::BTreeMap;

pub(crate) fn required(diagnostic: &Diagnostic) -> bool {
    diagnostic
        .identifier
        .split('.')
        .next()
        .is_some_and(|group| {
            matches!(
                group,
                "if" | "elseif"
                    | "while"
                    | "doWhile"
                    | "ternary"
                    | "booleanAnd"
                    | "booleanOr"
                    | "booleanNot"
                    | "logicalXor"
                    | "identical"
                    | "notIdentical"
                    | "equal"
                    | "notEqual"
                    | "smaller"
                    | "smallerOrEqual"
                    | "greater"
                    | "greaterOrEqual"
                    | "instanceof"
                    | "nullsafe"
                    | "nullCoalesce"
            )
        })
        || matches!(
            diagnostic.identifier.as_str(),
            "function.alreadyNarrowedType" | "function.impossibleType" | "foreach.emptyArray"
        )
}

pub(crate) fn retain_native(findings: &mut Vec<Diagnostic>, native: &[Diagnostic]) {
    let key = |finding: &Diagnostic| {
        (
            finding.file.clone(),
            finding.line,
            finding.identifier.clone(),
            finding.source_range,
        )
    };
    let mut confirmed = BTreeMap::new();
    for finding in native.iter().filter(|finding| required(finding)) {
        confirmed
            .entry(key(finding))
            .or_insert_with(Vec::new)
            .push(finding);
    }
    findings.retain_mut(|finding| {
        if !required(finding) {
            return true;
        }
        if let Some(replacement) = confirmed.get_mut(&key(finding)).and_then(Vec::pop) {
            finding.message.clone_from(&replacement.message);
            true
        } else {
            false
        }
    });
}
