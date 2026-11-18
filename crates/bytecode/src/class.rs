//! Immutable class declarations defer dependency linking and default evaluation to each request.

use crate::{ConstId, FunctionId, NameId, SymbolId, TypeId};

/// PHP visibility is retained independently of the mangled instance storage key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Public,
    Protected,
    Private,
}

/// Interfaces cannot be instantiated; traits and enums belong to later milestones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Interface,
}

/// A default is either immutable literal data or lazily evaluated bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Initializer {
    Literal(ConstId),
    Function(FunctionId),
}

/// One source declaration, before parents or interfaces are resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassDeclaration {
    pub metadata: crate::reflection::DeclarationMetadata,
    pub line: u32,
    pub name: String,
    pub parent: Option<SymbolId>,
    pub interfaces: Vec<SymbolId>,
    pub kind: ClassKind,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_readonly: bool,
    /// stdClass and AllowDynamicProperties opt out of dynamic-property deprecations.
    pub allows_dynamic_properties: bool,
    pub constants: Vec<ClassConstant>,
    pub properties: Vec<PropertyDeclaration>,
    pub methods: Vec<MethodDeclaration>,
}

/// A class constant computes its value on first access in the declaring scope.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassConstant {
    pub metadata: crate::reflection::DeclarationMetadata,
    pub name: NameId,
    pub visibility: Visibility,
    pub is_final: bool,
    pub initializer: FunctionId,
    /// PHP 8.3 validates the lazily evaluated value against this declaration.
    pub ty: Option<TypeId>,
}

/// An absent default on a typed property means uninitialized, rather than null.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDeclaration {
    pub promoted: bool,
    pub metadata: crate::reflection::DeclarationMetadata,
    pub name: NameId,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_readonly: bool,
    pub ty: Option<TypeId>,
    pub default: Option<Initializer>,
}

/// Methods reuse ordinary function bytecode and its parameter/return metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodDeclaration {
    pub name: NameId,
    pub function: FunctionId,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_final: bool,
}
