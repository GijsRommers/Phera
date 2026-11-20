//! `if ($this->service instanceof Resettable)` refines that property on the true path.
//! Reassigning the root or replacing the property invalidates its facts.

use super::{Scope, ValueType};
use phera_ast::{Arguments, Expression, ExpressionKind, MemberName, Variable};
use phera_source::Span;
use std::collections::{BTreeMap, BTreeSet};

// A PHP variable name cannot contain a colon.
pub(crate) const SELF_PROPERTIES: &str = ":self";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Place {
    pub root: String,
    pub members: Vec<AccessStep>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessStep {
    Property(String),
    Method(String),
    IntegerOffset(i64),
    StringOffset(Vec<u8>),
    VariableOffset(String),
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
                ExpressionKind::Index {
                    target,
                    index: Some(index),
                } => {
                    let step = match &index.kind {
                        ExpressionKind::Literal(phera_ast::Literal::Int(key)) => {
                            AccessStep::IntegerOffset(*key)
                        }
                        ExpressionKind::Literal(phera_ast::Literal::String(key)) => {
                            AccessStep::StringOffset(key.clone())
                        }
                        ExpressionKind::Variable(Variable::Named(key)) => {
                            AccessStep::VariableOffset(key.clone())
                        }
                        _ => return None,
                    };
                    members.push(step);
                    expression = target;
                }
                ExpressionKind::StaticProperty {
                    class,
                    name: Variable::Named(name),
                    ..
                } if matches!(&class.kind, ExpressionKind::Name(class)
                        if class.kind == phera_ast::NameKind::Unqualified
                        && class.text.eq_ignore_ascii_case("self")) =>
                {
                    members.push(AccessStep::Property(name.clone()));
                    members.reverse();
                    return Some(Self {
                        root: SELF_PROPERTIES.into(),
                        members,
                    });
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

    pub fn refers_to(&self, name: &str) -> bool {
        self.root == name
            || self.members.iter().any(
                |step| matches!(step, AccessStep::VariableOffset(variable) if variable == name),
            )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    pub ty: ValueType,
    pub narrowed_at: Option<Span>,
}

#[derive(Clone, Debug, Default)]
pub struct Facts {
    types: BTreeMap<Place, Fact>,
    null_relations: BTreeSet<NullRelation>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NullRelation {
    AtLeastOneNonNull(String, String),
    NonNullResult { result: String, receiver: String },
}

impl NullRelation {
    fn depends_on(&self, name: &str) -> bool {
        match self {
            Self::AtLeastOneNonNull(left, right) => left == name || right == name,
            Self::NonNullResult { result, receiver } => result == name || receiver == name,
        }
    }
}

impl Facts {
    pub fn is_empty(&self) -> bool {
        self.types.is_empty() && self.null_relations.is_empty()
    }

    pub fn get(&self, place: &Place) -> Option<&Fact> {
        self.types.get(place)
    }

    pub fn record(&mut self, place: Place, ty: ValueType, narrowed_at: Option<Span>) {
        self.types.insert(
            place,
            Fact {
                ty: ty.limit_complexity(),
                narrowed_at,
            },
        );
    }

    pub fn forget_variable(&mut self, name: &str) {
        self.types.retain(|place, _| !place.refers_to(name));
        self.null_relations
            .retain(|relation| !relation.depends_on(name));
    }

    pub fn forget_static(&mut self) {
        self.types.retain(|place, _| place.root != SELF_PROPERTIES);
    }

    pub fn forget_receiver(&mut self, receiver: &Place) {
        self.types.retain(|place, _| {
            place.root != receiver.root
                || place.members.len() <= receiver.members.len()
                || !place.members.starts_with(&receiver.members)
        });
    }

    pub fn forget_write(&mut self, target: Option<&Place>) {
        self.types
            .retain(|place, _| !affected_by_write(place, target));
    }

    pub fn remember_non_null_pair(&mut self, left: &str, right: &str) {
        if self.null_relations.len() >= 32 {
            return;
        }
        let (first, last) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        self.null_relations
            .insert(NullRelation::AtLeastOneNonNull(first.into(), last.into()));
    }

    pub fn remember_nullsafe_result(&mut self, result: &str, receiver: &str) {
        if self.null_relations.len() < 32 {
            self.null_relations.insert(NullRelation::NonNullResult {
                result: result.into(),
                receiver: receiver.into(),
            });
        }
    }

    pub fn known_non_null_when(&self, tested: &str, is_null: bool) -> Vec<String> {
        self.null_relations
            .iter()
            .filter_map(|relation| match relation {
                NullRelation::AtLeastOneNonNull(left, right) if is_null && left == tested => {
                    Some(right.clone())
                }
                NullRelation::AtLeastOneNonNull(left, right) if is_null && right == tested => {
                    Some(left.clone())
                }
                NullRelation::NonNullResult { result, receiver }
                    if !is_null && result == tested =>
                {
                    Some(receiver.clone())
                }
                _ => None,
            })
            .collect()
    }

    pub fn retain_common(&mut self, paths: &[Scope]) {
        self.types.retain(|place, fact| {
            for path in paths {
                let Some(other) = path.facts.get(place) else {
                    return false;
                };
                if other.ty != fact.ty {
                    let joined = fact.ty.join(&other.ty);
                    if joined.is_mixed() {
                        return false;
                    }
                    fact.ty = joined.limit_complexity();
                    fact.narrowed_at = None;
                }
                if other.narrowed_at != fact.narrowed_at {
                    fact.narrowed_at = None;
                }
            }
            true
        });
        self.null_relations.retain(|relation| {
            paths
                .iter()
                .all(|scope| scope.facts.null_relations.contains(relation))
        });
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
