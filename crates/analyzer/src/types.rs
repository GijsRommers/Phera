//! Values such as `3` refine `int`, and `Readable&Seekable` carries both member sets.
//! Keep common facts inline and share compound types between signatures and flow scopes.
//! Type compatibility distinguishes a proven rejection from insufficient information.

use crate::names::Names;
use phera_ast::{Type, TypeKind, UseKind};
use std::sync::Arc;

mod acceptance;
mod display;
mod substitution;

pub use acceptance::{Compatibility, TypePolicy};
pub use substitution::RelativeTypes;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueType {
    Mixed,
    Null,
    Bool,
    BoolLiteral(bool),
    Int,
    IntLiteral(i64),
    IntRange(i64, i64),
    Float,
    String,
    StringLiteral(Arc<[u8]>),
    StringRefinement(StringKind),
    Array,
    Iterable,
    Resource,
    Collection(Arc<Collection>),
    Shape(Arc<Shape>),
    Object,
    Void,
    Never,
    Class(String),
    Callable(Arc<crate::index::Signature>),
    ClassString(Arc<ValueType>),
    Generic(Arc<Generic>),
    Template(Arc<Template>),
    Projection(Arc<Projection>),
    TypeConstant(Arc<Constant>),
    Conditional(Arc<Conditional>),
    This,
    UnknownCallable,
    Union(Arc<[ValueType]>),
    Intersection(Arc<[ValueType]>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringKind {
    NonEmpty,
    Truthy,
    Numeric,
    Literal,
    Lowercase,
    Uppercase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionKind {
    List,
    Array,
    Iterable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collection {
    pub kind: CollectionKind,
    pub key: ValueType,
    pub value: ValueType,
    pub non_empty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShapeKey {
    Integer(i64),
    String(Arc<str>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub key: ShapeKey,
    pub value: ValueType,
    pub optional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    pub fields: Vec<Field>,
    pub remainder: Option<Collection>,
    pub kind: ShapeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Array,
    List,
    Object,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generic {
    pub base: String,
    pub arguments: Vec<ValueType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub bound: ValueType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Projection {
    Keys(ValueType),
    Values(ValueType),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constant {
    pub class: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conditional {
    pub variable: Option<String>,
    pub subject: ValueType,
    pub tested: ValueType,
    pub test: ConditionTest,
    pub yes: ValueType,
    pub no: ValueType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionTest {
    Is,
    IsNot,
}

impl ValueType {
    pub fn value_choices(&self) -> Self {
        if !matches!(self, Self::Conditional(_)) {
            return self.clone();
        }
        let mut pending = vec![self];
        let mut result = Self::Never;
        while let Some(ty) = pending.pop() {
            if let Self::Conditional(conditional) = ty {
                pending.extend([&conditional.no, &conditional.yes]);
            } else {
                result = result.join(ty);
            }
        }
        result
    }

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
        if matches!((self, other), (Self::BoolLiteral(left), Self::BoolLiteral(right)) if left != right)
            || matches!(
                (self, other),
                (Self::Bool, Self::BoolLiteral(_)) | (Self::BoolLiteral(_), Self::Bool)
            )
        {
            return Self::Bool;
        }
        if matches!(self, Self::Mixed) || matches!(other, Self::Mixed) {
            return Self::Mixed;
        }
        let mut values = match self {
            Self::Union(v) => v.to_vec(),
            _ => vec![self.clone()],
        };
        let incoming = match other {
            Self::Union(v) => v.to_vec(),
            _ => vec![other.clone()],
        };
        for value in incoming {
            if !values.contains(&value) {
                if values.len() >= 64 {
                    // Generated code can join thousands of distinct literal
                    // values. Keep that join bounded rather than storing an
                    // ever-growing list in every subsequent scope.
                    return Self::Mixed;
                }
                values.push(value);
            }
        }
        Self::Union(values.into())
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
            Self::Intersection(types) => types.iter().any(Self::excludes_null),
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
            Self::Union(types) | Self::Intersection(types) => {
                types.iter().any(Self::may_be_callable)
            }
            _ => false,
        }
    }

    pub fn intersection(left: &Self, right: &Self) -> Self {
        if left == right || right == &Self::Mixed {
            return left.clone();
        }
        if left == &Self::Mixed {
            return right.clone();
        }
        if left == &Self::Never || right == &Self::Never {
            return Self::Never;
        }
        let mut bounds = match left {
            Self::Intersection(bounds) => bounds.to_vec(),
            left => vec![left.clone()],
        };
        let incoming = match right {
            Self::Intersection(bounds) => bounds.as_ref(),
            right => std::slice::from_ref(right),
        };
        for bound in incoming {
            if !bounds.contains(bound) {
                bounds.push(bound.clone());
            }
        }
        Self::Intersection(bounds.into())
    }

    pub fn class_name(&self) -> Option<&str> {
        match self {
            Self::Class(name) => Some(name),
            Self::Generic(generic) => Some(&generic.base),
            _ => None,
        }
    }

    pub fn declared(ty: Option<&Type>, names: &Names) -> Self {
        let Some(ty) = ty else {
            return Self::Mixed;
        };
        enum Lower<'a> {
            Visit(&'a Type),
            Nullable,
            Combine { count: usize, intersection: bool },
        }
        let mut pending = vec![Lower::Visit(ty)];
        let mut values = Vec::new();
        while let Some(step) = pending.pop() {
            match step {
                Lower::Visit(ty) => match &ty.kind {
                    TypeKind::Named(name) => {
                        values.push(match name.text.to_ascii_lowercase().as_str() {
                            "null" => Self::Null,
                            "true" => Self::BoolLiteral(true),
                            "false" => Self::BoolLiteral(false),
                            "bool" => Self::Bool,
                            "int" => Self::Int,
                            "float" => Self::Float,
                            "string" => Self::String,
                            "array" => Self::Array,
                            "iterable" => Self::Iterable,
                            "object" => Self::Object,
                            "void" => Self::Void,
                            "never" => Self::Never,
                            "mixed" => Self::Mixed,
                            "callable" => Self::UnknownCallable,
                            "self" | "static" | "parent" => {
                                Self::Class(name.text.to_ascii_lowercase())
                            }
                            _ => Self::Class(names.resolve(name, UseKind::Class)[0].clone()),
                        });
                    }
                    TypeKind::Nullable(inner) => {
                        pending.push(Lower::Nullable);
                        pending.push(Lower::Visit(inner));
                    }
                    TypeKind::Union(items) | TypeKind::Intersection(items) => {
                        pending.push(Lower::Combine {
                            count: items.len(),
                            intersection: matches!(ty.kind, TypeKind::Intersection(_)),
                        });
                        pending.extend(items.iter().rev().map(Lower::Visit));
                    }
                },
                Lower::Nullable => {
                    if let Some(value) = values.pop() {
                        values.push(value.join(&Self::Null));
                    }
                }
                Lower::Combine {
                    count,
                    intersection,
                } => {
                    let start = values.len().saturating_sub(count);
                    let joined = values.drain(start..).reduce(|left, right| {
                        if intersection {
                            Self::intersection(&left, &right)
                        } else {
                            left.join(&right)
                        }
                    });
                    values.extend(joined);
                }
            }
        }
        values.pop().unwrap_or(Self::Mixed)
    }
}
