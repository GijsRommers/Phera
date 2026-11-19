//! Token checks for PER Coding Style, for example `else if` becomes `elseif`.

mod declarations;
mod lists;
mod tokens;

use super::rule::{Check, DefaultStatus, Group, Rule, Visit};

pub(super) fn rules() -> impl Iterator<Item = &'static Rule> {
    tokens::RULES
        .iter()
        .chain(declarations::RULES)
        .chain(lists::RULES)
}

const fn style(
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
        fix,
        group: Group::Style,
        visit,
        check: Some(check),
        default: DefaultStatus::OptIn,
    }
}
