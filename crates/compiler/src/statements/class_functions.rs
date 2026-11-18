//! \`class C { const N = 1; function read() { return self::N; } }\` keeps methods
//! and constant initializers in the file's ordinary function table.

use phera_ast as ast;
use phera_bytecode::Visibility;

use crate::ir::{ClassMethod, Instruction};
use crate::policies::DeclarationTiming;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// A separate pure function retains expression evaluation order without storing AST nodes.
    pub(super) fn compile_initializer(
        &mut self,
        expression: &ast::Expression,
        class: &str,
        trait_scope: Option<&str>,
        line: usize,
    ) -> Result<usize, CompileError> {
        if let Some(message) = crate::constant_callables::callable_constant_error(expression) {
            return Err(self.error(message, line));
        }
        if !crate::parameter_defaults::constant_expression(expression) {
            return Err(self.error("Constant expression contains invalid operations", line));
        }
        let mut builder = Compiler::new(self.file, self.lines, "{initializer}", &self.namespace);
        builder.class_scope = Some(class.to_owned());
        builder.trait_scope = trait_scope.map(str::to_owned);
        builder.imports = self.imports.clone();
        let result = builder.compile_expression(expression)?;
        builder.emit(Instruction::Return(result), line);
        Ok(self.append_function_body(&mut builder))
    }

    /// Reuse parameter/default lowering while keeping method names out of the function table.
    pub(super) fn compile_method(
        &mut self,
        class: &str,
        trait_scope: Option<&str>,
        method: &ast::Method,
        line: usize,
    ) -> Result<ClassMethod, CompileError> {
        let declaration = ast::FunctionDeclaration {
            attributes: method.attributes.clone(),
            name: ast::Identifier {
                name: format!("{class}::{}", method.name.name),
                span: method.name.span,
            },
            by_reference: method.by_reference,
            parameters: method.parameters.clone(),
            return_type: method.return_type.clone(),
            body: method.body.clone().unwrap_or_default(),
        };
        let index = self.functions.len();
        // The synthetic name is already fully qualified.
        let previous = std::mem::replace(&mut self.trait_scope, trait_scope.map(str::to_owned));
        let result = self.compile_function(&declaration, DeclarationTiming::Hoisted, line);
        self.trait_scope = previous;
        result?;
        self.hoisted.retain(|candidate| *candidate != index);
        Ok(ClassMethod {
            name: method.name.name.clone(),
            function: index,
            visibility: match method.modifiers.visibility {
                Some(ast::Visibility::Private) => Visibility::Private,
                Some(ast::Visibility::Protected) => Visibility::Protected,
                _ => Visibility::Public,
            },
            is_static: method.modifiers.is_static,
            is_abstract: method.body.is_none(),
            is_final: method.modifiers.is_final,
        })
    }
}
