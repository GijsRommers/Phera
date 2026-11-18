//! Immutable class declarations defer dependency linking and default evaluation to each request.

use crate::{ConstId, FunctionId, NameId, SymbolId, TypeId};

/// Internal class-name symbol for the lexical consuming class of a trait body.
pub const TRAIT_CLASS_NAME: &str = "\0trait-class";

/// PHP visibility is retained independently of the mangled instance storage key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Public,
    Protected,
    Private,
}

/// Anonymous declarations are linked once per source occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Interface,
    Anonymous,
    Trait,
}

/// Resolved names and source-order rules for `use T, U { T::m insteadof U; }`.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitUse {
    pub names: Vec<String>,
    pub adaptations: Vec<TraitAdaptation>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TraitAdaptation {
    Precedence {
        trait_name: String,
        method: String,
        instead_of: Vec<String>,
    },
    Alias {
        trait_name: Option<String>,
        method: String,
        alias: Option<String>,
        visibility: Option<Visibility>,
    },
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
    pub traits: Vec<TraitUse>,
}

/// A class constant computes its value on first access in the declaring scope.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassConstant {
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
