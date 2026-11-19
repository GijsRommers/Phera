//! Declaration data read by reflection, such as the argument in `#[Column('id')]`.

use std::{collections::HashMap, sync::Arc};

use phera_source::Span;

/// The file-local index of an expression that is only executed by reflection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitializerId(pub usize);

/// Keep literals in the file's cold metadata and compile expressions on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Initializer {
    Null,
    Bool(bool),
    Int(i64),
    Float(u64),
    String(Vec<u8>),
    EmptyArray,
    Expression(InitializerId),
}

/// Distinct PHP import tables for classes, functions and constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NameKind {
    Class,
    Function,
    Constant,
}

pub type Imports = Arc<HashMap<(NameKind, String), String>>;

/// Validated source with the lexical context needed to compile it on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializerSource {
    pub span: Span,
    pub first_line: usize,
    pub function: String,
    pub namespace: String,
    pub scope: Option<String>,
    pub imports: Option<Imports>,
    pub strict_types: bool,
}

/// Argument initializers execute in declaration order with the lexical scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub line: usize,
    pub strict_types: bool,
    pub validation_error: Option<String>,
    pub arguments: Vec<AttributeArgument>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeArgument {
    pub name: Option<String>,
    pub initializer: Initializer,
}

/// Source information shared by declarations and reflected members.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeclarationMetadata {
    pub span: Span,
    pub doc_comment: Span,
    pub attributes: Vec<Attribute>,
}

/// Parameter metadata indexed by its position in the function signature.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParameterMetadata {
    pub attributes: Vec<Attribute>,
    pub default: Option<Initializer>,
    pub default_constant: Option<String>,
    pub promoted: bool,
}

/// Metadata kept beside the compiled unit, outside the function and frame layouts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionMetadata {
    pub declaration: DeclarationMetadata,
    pub parameters: Vec<ParameterMetadata>,
}
