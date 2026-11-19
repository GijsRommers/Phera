//! Type diagnostics render PHP names, for example `int|string` and `list<User>`.

use super::*;
use std::fmt::{self, Display, Formatter};

enum Part<'a> {
    Type(&'a ValueType),
    Text(&'static str),
    Field(&'a Field, bool),
}

impl Display for ValueType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut parts = vec![Part::Type(self)];
        while let Some(part) = parts.pop() {
            let ty = match part {
                Part::Text(text) => {
                    f.write_str(text)?;
                    continue;
                }
                Part::Field(field, keyed) => {
                    if keyed {
                        match &field.key {
                            ShapeKey::Integer(key) => write!(f, "{key}")?,
                            ShapeKey::String(key)
                                if key
                                    .bytes()
                                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') =>
                            {
                                f.write_str(key)?
                            }
                            ShapeKey::String(key) => write!(f, "'{}'", key.escape_default())?,
                        }
                        if field.optional {
                            f.write_str("?")?;
                        }
                        f.write_str(": ")?;
                    }
                    &field.value
                }
                Part::Type(ty) => ty,
            };
            let name = match ty {
                Self::Mixed => "mixed",
                Self::Null => "null",
                Self::Bool => "bool",
                Self::BoolLiteral(true) => "true",
                Self::BoolLiteral(false) => "false",
                Self::Int => "int",
                Self::Float => "float",
                Self::String => "string",
                Self::Array => "array",
                Self::Iterable => "iterable",
                Self::Resource => "resource",
                Self::Object => "object",
                Self::Void => "void",
                Self::Never => "never",
                Self::UnknownCallable => "callable",
                Self::This => "$this",
                Self::StringRefinement(kind) => match kind {
                    StringKind::NonEmpty => "non-empty-string",
                    StringKind::Truthy => "non-falsy-string",
                    StringKind::Numeric => "numeric-string",
                    StringKind::Literal => "literal-string",
                    StringKind::Lowercase => "lowercase-string",
                    StringKind::Uppercase => "uppercase-string",
                },
                Self::Class(name) => name,
                Self::Template(template) => &template.name,
                Self::IntLiteral(value) => {
                    write!(f, "{value}")?;
                    continue;
                }
                Self::StringLiteral(bytes) => {
                    let text = String::from_utf8_lossy(bytes);
                    write!(f, "'{}'", text.escape_default())?;
                    continue;
                }
                Self::IntRange(min, max) => {
                    let min = if *min == i64::MIN {
                        "min".into()
                    } else {
                        min.to_string()
                    };
                    let max = if *max == i64::MAX {
                        "max".into()
                    } else {
                        max.to_string()
                    };
                    write!(f, "int<{min}, {max}>")?;
                    continue;
                }
                Self::ClassString(bound) => {
                    f.write_str("class-string")?;
                    if **bound != Self::Object {
                        f.write_str("<")?;
                        parts.extend([Part::Text(">"), Part::Type(bound)]);
                    }
                    continue;
                }
                Self::Generic(generic) => {
                    write!(f, "{}<", generic.base)?;
                    parts.push(Part::Text(">"));
                    sequence(&mut parts, &generic.arguments, ", ", false);
                    continue;
                }
                Self::Union(types) | Self::Intersection(types) => {
                    let union = matches!(ty, Self::Union(_));
                    if union
                        && let Some(names) =
                            types.iter().map(native_member).collect::<Option<Vec<_>>>()
                    {
                        f.write_str(&phera_bytecode::union_in_php_order(&names.join("|")))?;
                        continue;
                    }
                    sequence(&mut parts, types, if union { "|" } else { "&" }, union);
                    continue;
                }
                Self::Collection(collection) => {
                    if collection.non_empty {
                        f.write_str("non-empty-")?;
                    }
                    f.write_str(match collection.kind {
                        CollectionKind::Array => "array<",
                        CollectionKind::List => "list<",
                        CollectionKind::Iterable => "iterable<",
                    })?;
                    parts.extend([Part::Text(">"), Part::Type(&collection.value)]);
                    if collection.kind != CollectionKind::List {
                        parts.extend([Part::Text(", "), Part::Type(&collection.key)]);
                    }
                    continue;
                }
                Self::Shape(shape) => {
                    f.write_str(if shape.kind == ShapeKind::Object {
                        "object{"
                    } else {
                        "array{"
                    })?;
                    parts.push(Part::Text("}"));
                    if shape.remainder.is_some() {
                        parts.push(Part::Text("..."));
                        if !shape.fields.is_empty() {
                            parts.push(Part::Text(", "));
                        }
                    }
                    for (index, field) in shape.fields.iter().enumerate().rev() {
                        parts.push(Part::Field(
                            field,
                            shape.kind != ShapeKind::List || field.optional,
                        ));
                        if index != 0 {
                            parts.push(Part::Text(", "));
                        }
                    }
                    continue;
                }
                Self::Callable(_) => "Closure",
                Self::Projection(projection) => {
                    let (name, operand) = match projection.as_ref() {
                        Projection::Keys(operand) => ("key-of<", operand),
                        Projection::Values(operand) => ("value-of<", operand),
                    };
                    f.write_str(name)?;
                    parts.extend([Part::Text(">"), Part::Type(operand)]);
                    continue;
                }
                Self::TypeConstant(constant) => {
                    write!(f, "{}::{}", constant.class, constant.name)?;
                    continue;
                }
                Self::Conditional(conditional) => {
                    parts.extend([
                        Part::Type(&conditional.no),
                        Part::Text("|"),
                        Part::Type(&conditional.yes),
                    ]);
                    continue;
                }
            };
            f.write_str(name)?;
        }
        Ok(())
    }
}

fn native_member(ty: &ValueType) -> Option<&str> {
    Some(match ty {
        ValueType::Null => "null",
        ValueType::Bool => "bool",
        ValueType::BoolLiteral(false) => "false",
        ValueType::BoolLiteral(true) => "true",
        ValueType::Int => "int",
        ValueType::Float => "float",
        ValueType::String => "string",
        ValueType::Array => "array",
        ValueType::Iterable => "iterable",
        ValueType::Object => "object",
        ValueType::UnknownCallable => "callable",
        ValueType::Class(name) => name,
        // A PHPDoc type such as array<int|string> can contain a pipe of
        // its own. Only pass native members to the engine's native sorter.
        _ => return None,
    })
}

fn sequence<'a>(
    parts: &mut Vec<Part<'a>>,
    items: &'a [ValueType],
    separator: &'static str,
    parenthesize: bool,
) {
    for (index, item) in items.iter().enumerate().rev() {
        if index + 1 < items.len() {
            parts.push(Part::Text(separator));
        }
        if parenthesize && matches!(item, ValueType::Intersection(_)) {
            parts.extend([Part::Text(")"), Part::Type(item), Part::Text("(")]);
        } else {
            parts.push(Part::Type(item));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_members_use_the_same_order_as_runtime_diagnostics() {
        assert_eq!(
            ValueType::Int.join(&ValueType::String).to_string(),
            "string|int"
        );
        assert_eq!(ValueType::Int.join(&ValueType::Null).to_string(), "?int");
    }
}
