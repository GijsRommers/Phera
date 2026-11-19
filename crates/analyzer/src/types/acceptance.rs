//! `list<User>` fits an array parameter, and PHP can widen `int` to `float`.
//! Use explicit requirements for unions and intersections. Unknown facts stay uncertain.

use super::*;
use crate::index::Index;

mod leaf;
use leaf::atomic;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compatibility {
    Accepts,
    Rejects,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypePolicy {
    Documentation,
    Strict,
    WeakValue,
    Coercive,
}

enum Requirement {
    All,
    Any,
}

enum Step<'a> {
    Compare(&'a ValueType, &'a ValueType),
    Reduce(usize, Requirement),
}

impl ValueType {
    pub fn accepts(&self, given: &Self, index: &Index, policy: TypePolicy) -> Compatibility {
        use Compatibility::*;
        let mut pending = vec![Step::Compare(self, given)];
        let mut results = Vec::new();
        while let Some(step) = pending.pop() {
            match step {
                Step::Reduce(count, requirement) => {
                    let start = results.len().saturating_sub(count);
                    let mut items = results.drain(start..);
                    let (decisive, fallback) = match requirement {
                        Requirement::All => (Rejects, Accepts),
                        Requirement::Any => (Accepts, Rejects),
                    };
                    let mut result = fallback;
                    for item in &mut items {
                        if item == decisive {
                            result = decisive;
                            break;
                        }
                        if item == Uncertain {
                            result = Uncertain;
                        }
                    }
                    drop(items);
                    results.push(result);
                }
                Step::Compare(expected, given) => {
                    if matches!(expected, Self::Mixed)
                        || matches!(given, Self::Never)
                        || expected == given
                    {
                        results.push(Accepts);
                        continue;
                    }
                    match (expected, given) {
                        (_, Self::Mixed) => results.push(Uncertain),
                        (_, Self::Void) if policy != TypePolicy::Documentation => {
                            pending.push(Step::Compare(expected, &Self::Null));
                        }
                        (_, Self::Class(name))
                            if index.class(name).is_none()
                                && let Some(constant) = index.doc_global_constant(name)
                                && !matches!(constant, Self::Class(_)) =>
                        {
                            pending.push(Step::Compare(expected, constant));
                        }
                        (_, Self::Union(types)) => {
                            let requirement = if policy == TypePolicy::Documentation {
                                Requirement::All
                            } else {
                                Requirement::Any
                            };
                            pending.push(Step::Reduce(types.len(), requirement));
                            pending.extend(types.iter().map(|ty| Step::Compare(expected, ty)));
                        }
                        (_, Self::Template(template)) => {
                            pending.push(Step::Compare(expected, &template.bound))
                        }
                        (_, Self::Conditional(conditional)) => {
                            let requirement = if policy == TypePolicy::Documentation {
                                Requirement::All
                            } else {
                                Requirement::Any
                            };
                            pending.push(Step::Reduce(2, requirement));
                            pending.push(Step::Compare(expected, &conditional.yes));
                            pending.push(Step::Compare(expected, &conditional.no));
                        }
                        (Self::Union(types), _) => {
                            pending.push(Step::Reduce(types.len(), Requirement::Any));
                            pending.extend(types.iter().map(|ty| Step::Compare(ty, given)));
                        }
                        (Self::Conditional(conditional), _) => {
                            pending.push(Step::Reduce(2, Requirement::Any));
                            pending.push(Step::Compare(&conditional.yes, given));
                            pending.push(Step::Compare(&conditional.no, given));
                        }
                        (Self::Intersection(types), _) => {
                            pending.push(Step::Reduce(types.len(), Requirement::All));
                            pending.extend(types.iter().map(|ty| Step::Compare(ty, given)));
                        }
                        (_, Self::Intersection(types)) => {
                            pending.push(Step::Reduce(types.len(), Requirement::Any));
                            pending.extend(types.iter().map(|ty| Step::Compare(expected, ty)));
                        }
                        (Self::Template(template), _) => {
                            pending.push(Step::Compare(&template.bound, given))
                        }
                        (Self::ClassString(expected), Self::ClassString(given)) => {
                            pending.push(Step::Compare(expected, given));
                        }
                        (Self::Callable(expected), Self::Callable(given)) => {
                            let required = given
                                .parameters
                                .iter()
                                .filter(|parameter| !parameter.optional && !parameter.variadic)
                                .count();
                            let promises = expected
                                .parameters
                                .iter()
                                .filter(|parameter| !parameter.optional && !parameter.variadic)
                                .count();
                            if required > promises {
                                results.push(Rejects);
                                continue;
                            }
                            let count = expected.parameters.len().min(given.parameters.len());
                            pending.push(Step::Reduce(1 + count, Requirement::All));
                            pending.push(Step::Compare(&expected.returns, &given.returns));
                            pending.extend(
                                expected
                                    .parameters
                                    .iter()
                                    .zip(&given.parameters)
                                    .map(|(p, g)| Step::Compare(&g.ty, &p.ty)),
                            );
                        }
                        (Self::Collection(expected), Self::Collection(given))
                            if collection_accepts(expected, given, policy) =>
                        {
                            pending.push(Step::Reduce(2, Requirement::All));
                            pending.push(Step::Compare(&expected.key, &given.key));
                            pending.push(Step::Compare(&expected.value, &given.value));
                        }
                        (Self::Collection(expected), Self::Shape(given)) => {
                            if given.kind == ShapeKind::Object
                                || expected.kind == CollectionKind::List && !list_shape(given)
                                || expected.non_empty
                                    && given.fields.is_empty()
                                    && given.remainder.is_none()
                            {
                                results.push(Rejects);
                                continue;
                            }
                            let start = pending.len();
                            pending.push(Step::Reduce(0, Requirement::All));
                            for field in &given.fields {
                                let key = match field.key {
                                    ShapeKey::Integer(_) => &Self::Int,
                                    ShapeKey::String(_) => &Self::String,
                                };
                                pending.push(Step::Compare(&expected.key, key));
                                pending.push(Step::Compare(&expected.value, &field.value));
                            }
                            if let Some(remainder) = &given.remainder {
                                pending.push(Step::Compare(&expected.key, &remainder.key));
                                pending.push(Step::Compare(&expected.value, &remainder.value));
                            }
                            pending[start] =
                                Step::Reduce(pending.len() - start - 1, Requirement::All);
                        }
                        (Self::Shape(expected), Self::Shape(given)) => {
                            if (expected.kind == ShapeKind::Object)
                                != (given.kind == ShapeKind::Object)
                                || (expected.kind == ShapeKind::List && !list_shape(given))
                            {
                                results.push(Rejects);
                                continue;
                            }
                            let start = pending.len();
                            pending.push(Step::Reduce(0, Requirement::All));
                            if expected.fields.iter().all(|field| {
                                if let Some(other) =
                                    given.fields.iter().find(|other| other.key == field.key)
                                {
                                    pending.push(Step::Compare(&field.value, &other.value));
                                    true
                                } else {
                                    field.optional || given.remainder.is_some()
                                }
                            }) {
                                pending[start] =
                                    Step::Reduce(pending.len() - start - 1, Requirement::All);
                            } else {
                                pending.truncate(start);
                                results.push(Rejects);
                            }
                        }
                        (_, _) => results.push(atomic(expected, given, index, policy)),
                    }
                }
            }
        }
        results.pop().unwrap_or(Uncertain)
    }
}

fn collection_accepts(expected: &Collection, given: &Collection, policy: TypePolicy) -> bool {
    (!expected.non_empty || given.non_empty || policy != TypePolicy::Documentation)
        && (expected.kind == given.kind
            || expected.kind == CollectionKind::Iterable
            || expected.kind == CollectionKind::Array && given.kind == CollectionKind::List
            || expected.kind == CollectionKind::List
                && given.kind == CollectionKind::Array
                && policy != TypePolicy::Documentation)
}

fn list_shape(shape: &Shape) -> bool {
    shape.kind == ShapeKind::List
        || shape.remainder.is_none()
            && shape
                .fields
                .iter()
                .enumerate()
                .all(|(i, field)| field.key == ShapeKey::Integer(i as i64))
}
