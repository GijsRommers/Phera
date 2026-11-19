//! `if ($this->service instanceof Resettable)` refines that property on the true path.
//! Reassigning the root or replacing the property invalidates its facts.

use super::{Scope, ValueType};
use phera_ast::{Arguments, Expression, ExpressionKind, MemberName, Variable};
use phera_source::Span;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Place {
    pub root: String,
    pub members: Vec<AccessStep>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessStep {
    Property(String),
    Method(String),
}

impl Place {
    pub fn of(mut expression: &Expression) -> Option<Self> {
        let mut members = Vec::new();
        loop {
            match &expression.kind {
                ExpressionKind::Property {
                    target,
                    name: MemberName::Identifier(name),
                    nullsafe: false,
                } => {
                    members.push(AccessStep::Property(name.name.clone()));
                    expression = target;
                }
                ExpressionKind::MethodCall {
                    target,
                    name: MemberName::Identifier(name),
                    arguments: Arguments::List(arguments),
                    nullsafe: false,
                } if arguments.is_empty() => {
                    members.push(AccessStep::Method(name.name.to_ascii_lowercase()));
                    expression = target;
                }
                ExpressionKind::Variable(Variable::Named(root)) => {
                    members.reverse();
                    return Some(Self {
                        root: root.clone(),
                        members,
                    });
                }
                _ => return None,
            }
        }
    }

    pub fn variable(name: &str) -> Self {
        Self {
            root: name.into(),
            members: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    pub ty: ValueType,
    pub narrowed_at: Option<Span>,
}

#[derive(Clone, Debug, Default)]
pub struct Facts(BTreeMap<Place, Fact>);

impl Facts {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, place: &Place) -> Option<&Fact> {
        self.0.get(place)
    }

    pub fn record(&mut self, place: Place, ty: ValueType, narrowed_at: Option<Span>) {
        self.0.insert(place, Fact { ty, narrowed_at });
    }

    pub fn forget_variable(&mut self, name: &str) {
        self.0.retain(|place, _| place.root != name);
    }

    pub fn forget_receiver(&mut self, receiver: &Place) {
        self.0.retain(|place, _| {
            place.root != receiver.root
                || place.members.len() <= receiver.members.len()
                || !place.members.starts_with(&receiver.members)
        });
    }

    pub fn forget_write(&mut self, target: Option<&Place>) {
        self.0.retain(|place, _| !affected_by_write(place, target));
    }

    pub fn retain_common(&mut self, paths: &[Scope]) {
        self.0
            .retain(|place, fact| paths.iter().all(|path| path.facts.get(place) == Some(fact)));
    }
}

pub fn affected_by_write(place: &Place, target: Option<&Place>) -> bool {
    if place.members.is_empty() {
        return false;
    }
    let Some(written) = target.and_then(|target| target.members.last()) else {
        return true;
    };
    // `$alias = $this` can allow a write through a different root name.
    // A getter's value can also depend on a property on the same receiver.
    place.members.contains(written)
        || target.is_some_and(|target| {
            target.root == place.root
                && place
                    .members
                    .iter()
                    .any(|step| matches!(step, AccessStep::Method(_)))
        })
}
