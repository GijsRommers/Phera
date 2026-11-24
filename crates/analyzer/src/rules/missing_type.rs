//! `function load($ids): array` lacks a parameter type and an iterable value type.

use crate::{
    index::Index,
    types::{CollectionKind, ValueType},
};

pub(crate) struct MissingType {
    pub identifier: &'static str,
    pub message: String,
}

pub(crate) fn hints(
    ty: &ValueType,
    label: &str,
    kind: &'static str,
    index: &Index,
) -> Vec<MissingType> {
    if *ty == ValueType::Mixed {
        let ending = match kind {
            "missingType.parameter" => "with no type specified.",
            "missingType.return" => "has no return type specified.",
            _ => "has no type specified.",
        };
        return vec![MissingType {
            identifier: kind,
            message: format!("{label} {ending}"),
        }];
    }

    let subject = match kind {
        "missingType.return" => format!("{label} return type"),
        "missingType.property" => format!("{label} type"),
        _ => label.into(),
    };
    let mut result = Vec::new();
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        let iterable = match ty {
            ValueType::Array => Some("array"),
            ValueType::Iterable => Some("iterable"),
            ValueType::Collection(collection) if collection.value == ValueType::Mixed => {
                Some(match collection.kind {
                    CollectionKind::Array => "array",
                    CollectionKind::Iterable => "iterable",
                    CollectionKind::List => "list",
                })
            }
            ValueType::Collection(collection) => {
                pending.push(&collection.value);
                None
            }
            ValueType::Union(arms) => {
                let typed_array = arms.iter().any(|arm| match arm {
                    ValueType::Collection(collection) => {
                        collection.kind != CollectionKind::Iterable
                            && collection.value != ValueType::Mixed
                    }
                    ValueType::Shape(shape) => shape.kind != crate::types::ShapeKind::Object,
                    _ => false,
                });
                pending.extend(
                    arms.iter()
                        .filter(|arm| **arm != ValueType::Array || !typed_array),
                );
                None
            }
            ValueType::Intersection(arms) => {
                pending.extend(arms.iter());
                None
            }
            ValueType::Class(name) => {
                if let Some(class) = index.class(name)
                    && !class.documented.template_names.is_empty()
                {
                    let class_kind = match class.kind {
                        phera_ast::ClassKind::Interface => "interface",
                        phera_ast::ClassKind::Trait => "trait",
                        _ => "class",
                    };
                    let templates = class.documented.template_names.join(", ");
                    let verb = if kind == "missingType.parameter" {
                        "but "
                    } else {
                        ""
                    };
                    result.push(MissingType {
                        identifier: "missingType.generics",
                        message: format!(
                            "{subject} with generic {class_kind} {} {verb}does not specify its types: {templates}",
                            class.name,
                        ),
                    });
                }
                None
            }
            _ => None,
        };
        if let Some(iterable) = iterable {
            let verb = if kind == "missingType.parameter" {
                "with"
            } else {
                "has"
            };
            result.push(MissingType {
                identifier: "missingType.iterableValue",
                message: format!(
                    "{subject} {verb} no value type specified in iterable type {iterable}."
                ),
            });
        }
    }
    result
}
