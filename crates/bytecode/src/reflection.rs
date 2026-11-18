//! Declaration data read by reflection, such as the argument in `#[Column('id')]`.

use crate::FunctionId;

/// Argument initializers execute in declaration order with the lexical scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub validation_error: Option<String>,
    pub arguments: Vec<AttributeArgument>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeArgument {
    pub name: Option<String>,
    pub initializer: FunctionId,
}

/// Source information shared by declarations and reflected members.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeclarationMetadata {
    pub start_line: u32,
    pub end_line: u32,
    pub doc_comment: Option<Vec<u8>>,
    pub attributes: Vec<Attribute>,
}

/// Parameter metadata indexed by its position in the function signature.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParameterMetadata {
    pub attributes: Vec<Attribute>,
    pub default: Option<FunctionId>,
    pub default_constant: Option<String>,
    pub promoted: bool,
}

/// Metadata kept beside the compiled unit, outside the function and frame layouts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionMetadata {
    pub declaration: DeclarationMetadata,
    pub parameters: Vec<ParameterMetadata>,
}

impl FunctionMetadata {
    /// Move cold initializer references with the ordinary bytecode they name.
    pub fn relocate(&mut self, offset: usize) {
        self.declaration.relocate(offset);
        for parameter in &mut self.parameters {
            for attribute in &mut parameter.attributes {
                attribute.relocate(offset);
            }
            if let Some(default) = &mut parameter.default {
                default.0 += offset as u32;
            }
        }
    }
}

impl DeclarationMetadata {
    pub fn relocate(&mut self, offset: usize) {
        for attribute in &mut self.attributes {
            attribute.relocate(offset);
        }
    }
}

impl Attribute {
    fn relocate(&mut self, offset: usize) {
        for argument in &mut self.arguments {
            argument.initializer.0 += offset as u32;
        }
    }
}
