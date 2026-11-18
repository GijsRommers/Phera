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
    Enum,
}

impl ClassKind {
    pub fn is_class(self) -> bool {
        matches!(self, Self::Class | Self::Anonymous | Self::Enum)
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Class | Self::Anonymous => "Class",
            Self::Interface => "Interface",
            Self::Enum => "Enum",
            Self::Trait => "Trait",
        }
    }
}

/// `enum Status: int` and `enum Status: string` use different lookup keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumBacking {
    Int,
    String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumCase {
    pub name: NameId,
    pub backing: Option<FunctionId>,
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
        is_final: bool,
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
    pub traits: Vec<TraitUse>,
    pub enum_backing: Option<EnumBacking>,
    pub enum_cases: Vec<EnumCase>,
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
    pub is_final: bool,
    pub ty: Option<TypeId>,
    pub default: Option<Initializer>,
    pub hooks: Option<Box<PropertyHooks<FunctionId>>>,
}

/// Hook functions use regular function IDs but do not appear in the method table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyHooks<F> {
    pub get: Option<PropertyHook<F>>,
    pub set: Option<PropertyHook<F>>,
    pub is_virtual: bool,
    pub set_visibility: Option<Visibility>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyHook<F> {
    pub function: F,
    pub is_abstract: bool,
    pub is_final: bool,
    pub attributes: Vec<String>,
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
