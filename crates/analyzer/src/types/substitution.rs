//! For `Box<User>`, a member returning `list<T>` returns `list<User>`.
//! The work list rewrites structured types without descending the Rust call stack.

use super::*;
use std::collections::BTreeMap;

const MAX_TYPE_DEPTH: usize = 48;

#[derive(Clone, Copy)]
pub struct RelativeTypes<'a> {
    pub receiver: &'a ValueType,
    pub owner: &'a str,
    pub parent: Option<&'a str>,
}

impl RelativeTypes<'_> {
    fn class(&self, name: &str) -> Option<&str> {
        match name {
            "self" => Some(self.owner),
            "parent" => self.parent,
            "static" => self.receiver.class_name(),
            _ => None,
        }
    }
}

enum Step<'a> {
    Visit(&'a ValueType, usize),
    Collection(&'a Collection),
    Shape(&'a Shape),
    Generic(&'a Generic),
    Union(usize),
    Intersection(usize),
    ClassString,
    Conditional(&'a Conditional),
}

impl ValueType {
    pub fn substitute(
        &self,
        templates: &BTreeMap<String, Self>,
        relative: Option<RelativeTypes<'_>>,
    ) -> Self {
        if !matches!(
            self,
            Self::Collection(_)
                | Self::Shape(_)
                | Self::ClassString(_)
                | Self::Generic(_)
                | Self::Union(_)
                | Self::Intersection(_)
                | Self::Template(_)
                | Self::Conditional(_)
                | Self::Class(_)
                | Self::This
        ) {
            return self.clone();
        }
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
                            relative
                                .filter(|relative| fits(relative.receiver, depth))
                                .map(|relative| relative.receiver.clone())
                                .unwrap_or_else(|| ty.clone())
                        }
                        Self::Class(name) => relative
                            .and_then(|relative| {
                                relative.class(name).map(|name| Self::Class(name.into()))
                            })
                            .unwrap_or_else(|| ty.clone()),
                        Self::Collection(collection) => {
                            pending.push(Step::Collection(collection));
                            pending.extend([
                                Step::Visit(&collection.value, depth),
                                Step::Visit(&collection.key, depth),
                            ]);
                            continue;
                        }
                        Self::Shape(shape) => {
                            pending.push(Step::Shape(shape));
                            pending.extend(
                                shape
                                    .fields
                                    .iter()
                                    .rev()
                                    .map(|field| Step::Visit(&field.value, depth)),
                            );
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
                Step::Shape(shape) => {
                    let values =
                        results.split_off(results.len().saturating_sub(shape.fields.len()));
                    let fields = shape
                        .fields
                        .iter()
                        .zip(values)
                        .map(|(field, value)| Field {
                            value,
                            ..field.clone()
                        })
                        .collect();
                    Self::Shape(Arc::new(Shape {
                        fields,
                        ..shape.clone()
                    }))
                }
                Step::Generic(generic) => {
                    let arguments =
                        results.split_off(results.len().saturating_sub(generic.arguments.len()));
                    Self::Generic(Arc::new(Generic {
                        base: relative
                            .and_then(|relative| relative.class(&generic.base).map(str::to_string))
                            .unwrap_or_else(|| generic.base.clone()),
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
            ValueType::Shape(shape) => {
                pending.extend(shape.fields.iter().map(|field| (&field.value, next)))
            }
            ValueType::Template(template) => pending.push((&template.bound, next)),
            _ => {}
        }
    }
    true
}
