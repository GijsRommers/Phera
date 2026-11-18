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
    UnknownCallable,
    Union(Vec<ValueType>),
}

impl ValueType {
    pub fn join(&self, other: &Self) -> Self {
        if matches!(self, Self::Never) {
            return other.clone();
        }
        if matches!(other, Self::Never) {
            return self.clone();
        }
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

    pub fn without_null(&self) -> Self {
        match self {
            Self::Union(types) => types
                .iter()
                .filter(|ty| **ty != Self::Null)
                .cloned()
                .reduce(|left, right| left.join(&right))
                .unwrap_or(Self::Never),
            Self::Null => Self::Never,
            _ => self.clone(),
        }
    }

    pub fn excludes_null(&self) -> bool {
        match self {
            Self::Null | Self::Mixed | Self::Void => false,
            Self::Union(types) => types.iter().all(Self::excludes_null),
            _ => true,
        }
    }

    pub fn parameter(parameter: &phera_ast::Parameter, names: &Names) -> Self {
        let ty = Self::declared(parameter.ty.as_ref(), names);
        if matches!(
            parameter.default.as_ref().map(|value| &value.kind),
            Some(phera_ast::ExpressionKind::Literal(phera_ast::Literal::Null))
        ) {
            ty.join(&Self::Null)
        } else {
            ty
        }
    }

    pub fn may_be_callable(&self) -> bool {
        match self {
            Self::Callable(_) | Self::UnknownCallable => true,
            Self::Union(types) => types.iter().any(Self::may_be_callable),
            _ => false,
        }
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
                    "mixed" => Self::Mixed,
                    "callable" => Self::UnknownCallable,
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
