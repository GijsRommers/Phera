//! Map `positive-int`, `class-string<User>` and `array<K,V>` to checker types.

use super::*;

pub fn from_name(
    word: &str,
    name: &str,
    arguments: Vec<ValueType>,
    names: &Names,
    templates: &Templates,
) -> ValueType {
    use CollectionKind::*;
    use ValueType as T;
    let first = || arguments.first().cloned().unwrap_or(T::Mixed);
    match name {
        "mixed" => T::ExplicitMixed,
        "non-empty-mixed" | "empty" => T::Mixed,
        "never" | "never-return" | "never-returns" | "no-return" | "noreturn" => T::Never,
        "void" => T::Void,
        "null" => T::Null,
        "bool" | "boolean" => T::Bool,
        "true" => T::BoolLiteral(true),
        "false" => T::BoolLiteral(false),
        "int" | "integer" if arguments.is_empty() => T::Int,
        "int" => match arguments.as_slice() {
            [min, max] => match (bound(min, i64::MIN), bound(max, i64::MAX)) {
                (Some(min), Some(max)) if min <= max => T::IntRange(min, max),
                _ => T::Int,
            },
            _ => T::Int,
        },
        "min" | "max" => T::Class(word.into()),
        "int-mask" | "int-mask-of" => T::Int,
        "positive-int" => T::IntRange(1, i64::MAX),
        "non-negative-int" => T::IntRange(0, i64::MAX),
        "negative-int" => T::IntRange(i64::MIN, -1),
        "non-positive-int" => T::IntRange(i64::MIN, 0),
        "non-zero-int" => T::IntRange(i64::MIN, -1).join(&T::IntRange(1, i64::MAX)),
        "float" | "double" | "real" | "finite-float" | "positive-float" | "negative-float" => {
            T::Float
        }
        "string" => T::String,
        "non-empty-string" => T::StringRefinement(StringKind::NonEmpty),
        "non-falsy-string" | "truthy-string" => T::StringRefinement(StringKind::Truthy),
        "numeric-string" => T::StringRefinement(StringKind::Numeric),
        "literal-string" => T::StringRefinement(StringKind::Literal),
        "lowercase-string" => T::StringRefinement(StringKind::Lowercase),
        "uppercase-string" => T::StringRefinement(StringKind::Uppercase),
        "class-string" | "interface-string" | "trait-string" | "enum-string" => {
            T::ClassString(Arc::new(arguments.first().cloned().unwrap_or(T::Object)))
        }
        "array-key" => T::Int.join(&T::String).benevolent(),
        "number" => T::Int.join(&T::Float),
        "numeric" => T::Int
            .join(&T::Float)
            .join(&T::StringRefinement(StringKind::Numeric)),
        "scalar" | "empty-scalar" | "non-empty-scalar" => {
            T::Bool.join(&T::Int).join(&T::Float).join(&T::String)
        }
        "array" | "associative-array" if arguments.is_empty() => T::Array,
        "array" | "associative-array" | "non-empty-array" | "non-empty-associative-array" => {
            collection(Array, name.starts_with("non-empty"), arguments)
        }
        "list" | "non-empty-list" => collection(List, name.starts_with("non-empty"), arguments),
        "iterable" if arguments.is_empty() => T::Iterable,
        "iterable" | "non-empty-iterable" => {
            collection(Iterable, name.starts_with("non-empty"), arguments)
        }
        "key-of" => T::Projection(Arc::new(Projection::Keys(first()))),
        "value-of" => T::Projection(Arc::new(Projection::Values(first()))),
        "resource" | "closed-resource" | "open-resource" => T::Resource,
        "object" => T::Object,
        "callable" | "callable-object" | "callable-array" | "callable-string" | "pure-callable" => {
            T::UnknownCallable
        }
        _ if word.contains('-') => approximate(name),
        _ => {
            if let Some(alias) = templates.aliases.get(word) {
                return if templates.imported_aliases.contains(word) {
                    alias.clone()
                } else {
                    T::Template(Arc::new(Template {
                        name: word.into(),
                        bound: alias.clone(),
                    }))
                };
            }
            if let Some(bound) = templates.parameters.get(word) {
                return T::Template(Arc::new(Template {
                    name: word.into(),
                    bound: bound.clone(),
                }));
            }
            let base = super::super::class_name(word, names).unwrap_or_else(|| word.into());
            if arguments.is_empty() {
                T::Class(base)
            } else {
                T::Generic(Arc::new(Generic { base, arguments }))
            }
        }
    }
}

fn approximate(name: &str) -> ValueType {
    match name.rsplit('-').next() {
        Some("string") => ValueType::String,
        Some("int") => ValueType::Int,
        Some("float") => ValueType::Float,
        Some("array" | "list") => ValueType::Array,
        Some("object") => ValueType::Object,
        Some("resource") => ValueType::Resource,
        _ => ValueType::Mixed,
    }
}

fn bound(ty: &ValueType, unbounded: i64) -> Option<i64> {
    match ty {
        ValueType::IntLiteral(value) => Some(*value),
        ValueType::Class(name) if matches!(name.as_str(), "min" | "max") => Some(unbounded),
        _ => None,
    }
}

pub fn collection(kind: CollectionKind, non_empty: bool, arguments: Vec<ValueType>) -> ValueType {
    let key = match kind {
        CollectionKind::List => ValueType::Int,
        CollectionKind::Iterable => ValueType::Mixed,
        CollectionKind::Array => ValueType::Int.join(&ValueType::String).benevolent(),
    };
    let (key, value) = match arguments.as_slice() {
        [value] => (key, value.clone()),
        [key, value, ..] => (key.clone(), value.clone()),
        _ => (key, ValueType::Mixed),
    };
    ValueType::Collection(Arc::new(Collection {
        kind,
        key,
        value,
        non_empty,
    }))
}
