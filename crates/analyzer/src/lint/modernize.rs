//! Modern syntax and APIs such as `$value = $input ?? 'default'`.

mod callables;
mod declarations;
mod deprecations;
mod expressions;

use super::rule::{Check, DefaultStatus, Group, Rule, Visit};

pub(super) fn rules() -> impl Iterator<Item = &'static Rule> {
    expressions::RULES
        .iter()
        .chain(callables::RULES)
        .chain(declarations::RULES)
        .chain(deprecations::RULES)
}

const fn rule(
    name: &'static str,
    description: &'static str,
    bad: &'static str,
    good: &'static str,
    fix: &'static str,
    visit: Visit,
    check: Check,
) -> Rule {
    Rule {
        name,
        description,
        bad,
        good,
        group: Group::Modernize,
        default: DefaultStatus::OptIn,
        fix,
        visit,
        check: Some(check),
    }
}
