//! `@phpstan-assert User $this->user` promises a receiver property after a call.

use super::type_parser::{Templates, parse};
use crate::{names::Names, types::ValueType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assertion {
    pub branch: AssertionBranch,
    pub negated: bool,
    pub root: String,
    pub properties: Vec<String>,
    pub ty: ValueType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssertionBranch {
    Always,
    OnResult(bool),
}

pub fn parse_assertion(
    body: &str,
    names: &Names,
    templates: &Templates,
    branch: AssertionBranch,
) -> Option<Assertion> {
    let body = body.trim_start_matches('=');
    let negated = body.starts_with('!');
    let parsed = parse(body.trim_start_matches('!'), names, templates).ok()?;
    let subject = parsed.suffix.split_whitespace().next()?;
    let mut path = subject.strip_prefix('$')?.split("->");
    let root = path.next()?.to_owned();
    let properties: Vec<_> = path.map(String::from).collect();
    if parsed.ty.is_mixed() || !identifier(&root) || !properties.iter().all(|name| identifier(name))
    {
        return None;
    }
    Some(Assertion {
        branch,
        negated,
        root,
        properties,
        ty: parsed.ty,
    })
}

fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}
