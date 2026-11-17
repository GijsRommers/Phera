//! The initial type lattice: `$x = new DateTime;` records a class, not `mixed`.
//! Unknown facts join to mixed; unions retain only information true on all paths.

use crate::names::Names;
use phera_ast::{Type, TypeKind, UseKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueType {
    Mixed,
    Null,
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
    Void,
    Never,
    Class(String),
    Callable(Box<crate::index::Signature>),
    Union(Vec<ValueType>),
}

impl ValueType {
    pub fn join(&self, other: &Self) -> Self {
        if self == other {
            return self.clone();
        }
        if matches!(self, Self::Mixed) || matches!(other, Self::Mixed) {
            return Self::Mixed;
        }
        let mut values = match self {
            Self::Union(v) => v.clone(),
            _ => vec![self.clone()],
        };
        let incoming = match other {
            Self::Union(v) => v.clone(),
            _ => vec![other.clone()],
        };
        for value in incoming {
            if !values.contains(&value) {
                values.push(value);
            }
        }
        Self::Union(values)
    }

    pub fn declared(ty: Option<&Type>, names: &Names) -> Self {
        let Some(ty) = ty else {
            return Self::Mixed;
        };
        let mut pending = vec![ty];
        let mut result: Option<Self> = None;
        while let Some(ty) = pending.pop() {
            let next = match &ty.kind {
                TypeKind::Named(name) => match name.text.to_ascii_lowercase().as_str() {
                    "null" => Self::Null,
                    "true" | "false" | "bool" => Self::Bool,
                    "int" => Self::Int,
                    "float" => Self::Float,
                    "string" => Self::String,
                    "array" | "iterable" => Self::Array,
                    "object" => Self::Object,
                    "void" => Self::Void,
                    "never" => Self::Never,
                    "mixed" | "callable" => Self::Mixed,
                    "self" | "static" | "parent" => Self::Class(name.text.to_ascii_lowercase()),
                    _ => Self::Class(names.resolve(name, UseKind::Class)[0].clone()),
                },
                TypeKind::Nullable(inner) => {
                    pending.push(inner);
                    Self::Null
                }
                TypeKind::Union(items) => {
                    pending.extend(items);
                    continue;
                }
                TypeKind::Intersection(_) => return Self::Mixed,
            };
            result = Some(result.map_or(next.clone(), |old| old.join(&next)));
        }
        result.unwrap_or(Self::Mixed)
    }
}
