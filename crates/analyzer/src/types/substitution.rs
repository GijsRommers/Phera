//! For `Box<User>`, a member returning `list<T>` returns `list<User>`.
//! The work list rewrites structured types without descending the Rust call stack.

use super::*;
use std::collections::BTreeMap;

const MAX_TYPE_DEPTH: usize = 48;

enum Step<'a> {
    Visit(&'a ValueType, usize),
    Collection(&'a Collection),
    Generic(&'a Generic),
    Union(usize),
    Intersection(usize),
    ClassString,
    Conditional(&'a Conditional),
}

impl ValueType {
    pub fn substitute(&self, templates: &BTreeMap<String, Self>, receiver: Option<&Self>) -> Self {
        let mut pending = vec![Step::Visit(self, 0)];
        let mut results = Vec::new();
        while let Some(step) = pending.pop() {
            let value = match step {
                Step::Visit(ty, depth) => {
                    if depth >= MAX_TYPE_DEPTH {
                        results.push(Self::Mixed);
                        continue;
                    }
                    let depth = depth + 1;
                    match ty {
                        Self::Template(template) => templates
                            .get(&template.name)
                            .filter(|replacement| fits(replacement, depth))
                            .cloned()
                            .unwrap_or_else(|| ty.clone()),
                        Self::This | Self::Class(_)
                            if matches!(ty, Self::This)
                                || ty.class_name().is_some_and(|name| name == "static") =>
                        {
                            receiver
                                .filter(|receiver| fits(receiver, depth))
                                .cloned()
                                .unwrap_or_else(|| ty.clone())
                        }
                        Self::Collection(collection) => {
                            pending.push(Step::Collection(collection));
                            pending.extend([
                                Step::Visit(&collection.value, depth),
                                Step::Visit(&collection.key, depth),
                            ]);
                            continue;
                        }
                        Self::ClassString(bound) => {
                            pending.extend([Step::ClassString, Step::Visit(bound, depth)]);
                            continue;
                        }
                        Self::Generic(generic) => {
                            pending.push(Step::Generic(generic));
                            pending.extend(
                                generic
                                    .arguments
                                    .iter()
                                    .rev()
                                    .map(|arg| Step::Visit(arg, depth)),
                            );
                            continue;
                        }
                        Self::Union(types) | Self::Intersection(types) => {
                            pending.push(if matches!(ty, Self::Union(_)) {
                                Step::Union(types.len())
                            } else {
                                Step::Intersection(types.len())
                            });
                            pending.extend(types.iter().rev().map(|ty| Step::Visit(ty, depth)));
                            continue;
                        }
                        Self::Conditional(conditional) => {
                            pending.push(Step::Conditional(conditional));
                            pending.extend(
                                [
                                    &conditional.no,
                                    &conditional.yes,
                                    &conditional.tested,
                                    &conditional.subject,
                                ]
                                .map(|ty| Step::Visit(ty, depth)),
                            );
                            continue;
                        }
                        _ => ty.clone(),
                    }
                }
                Step::Collection(collection) => {
                    let value = results.pop().unwrap_or(Self::Mixed);
                    let key = results.pop().unwrap_or(Self::Mixed);
                    Self::Collection(Arc::new(Collection {
                        key,
                        value,
                        ..collection.clone()
                    }))
                }
                Step::Generic(generic) => {
                    let arguments =
                        results.split_off(results.len().saturating_sub(generic.arguments.len()));
                    let base = if generic.base == "static" {
                        receiver.and_then(Self::class_name).unwrap_or(&generic.base)
                    } else {
                        &generic.base
                    };
                    Self::Generic(Arc::new(Generic {
                        base: base.into(),
                        arguments,
                    }))
                }
                Step::ClassString => {
                    Self::ClassString(Arc::new(results.pop().unwrap_or(Self::Object)))
                }
                Step::Union(count) => results
                    .drain(results.len().saturating_sub(count)..)
                    .reduce(|left, right| left.join(&right))
                    .unwrap_or(Self::Never),
                Step::Intersection(count) => results
                    .drain(results.len().saturating_sub(count)..)
                    .reduce(|left, right| Self::intersection(&left, &right))
                    .unwrap_or(Self::Mixed),
                Step::Conditional(conditional) => {
                    let no = results.pop().unwrap_or(Self::Mixed);
                    let yes = results.pop().unwrap_or(Self::Mixed);
                    let tested = results.pop().unwrap_or(Self::Mixed);
                    let subject = results.pop().unwrap_or(Self::Mixed);
                    Self::Conditional(Arc::new(Conditional {
                        subject,
                        tested,
                        yes,
                        no,
                        ..conditional.clone()
                    }))
                }
            };
            results.push(value);
        }
        results.pop().unwrap_or(Self::Mixed)
    }
}

fn fits(ty: &ValueType, depth: usize) -> bool {
    let mut pending = vec![(ty, depth)];
    let mut remaining = 128;
    while let Some((ty, depth)) = pending.pop() {
        if depth >= MAX_TYPE_DEPTH || remaining == 0 {
            return false;
        }
        remaining -= 1;
        let next = depth + 1;
        match ty {
            ValueType::Generic(generic) => {
                pending.extend(generic.arguments.iter().map(|arg| (arg, next)))
            }
            ValueType::Union(arms) | ValueType::Intersection(arms) => {
                pending.extend(arms.iter().map(|arm| (arm, next)))
            }
            ValueType::ClassString(bound) => pending.push((bound, next)),
            ValueType::Collection(collection) => {
                pending.extend([(&collection.key, next), (&collection.value, next)])
            }
            ValueType::Template(template) => pending.push((&template.bound, next)),
            _ => {}
        }
    }
    true
}
