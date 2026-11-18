//! `class C { const N = 1; function read() { return self::N; } }` keeps methods
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
        let mut builder = Compiler::new(
            self.file,
            self.source,
            self.lines,
            "{initializer}",
            &self.namespace,
            self.compile_diagnostics,
        );
        builder.anonymous_classes = self.anonymous_classes.clone();
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
        if method.modifiers.is_final
            && method.modifiers.visibility == Some(ast::Visibility::Private)
            && !method.name.name.eq_ignore_ascii_case("__construct")
        {
            self.declaration_warnings
                .push(phera_bytecode::DeclarationWarning {
                message:
                    "Private methods cannot be final as they are never overridden by other classes"
                        .into(),
                line: line as u32,
            });
        }
        if trait_scope.is_some() {
            for parameter in &method.parameters {
                let Some(ty) = &parameter.ty else {
                    continue;
                };
                if !matches!(
                    parameter.default.as_ref().map(|default| &default.kind),
                    Some(ast::ExpressionKind::Literal(ast::Literal::Null))
                ) {
                    continue;
                }
                let ty = self.declaration_type(ty);
                if !ty.starts_with('?')
                    && !ty.split('|').any(|part| matches!(part, "null" | "mixed"))
                {
                    return Err(self.error(
                        format!("Implicitly marking parameter ${} of {class}::{}() as nullable is not supported by Phera: deprecated since PHP 8.4", parameter.name, method.name.name),
                        self.lines.line(parameter.span.start),
                    ));
                }
            }
        }
        let declaration = ast::FunctionDeclaration {
            attributes: method.attributes.clone(),
            name: ast::Identifier {
                name: format!("{class}::{}", method.name.name),
                span: method.name.span,
            },
            by_reference: method.by_reference,
            is_generator: method.is_generator,
            parameters: method.parameters.clone(),
            return_type: method.return_type.clone().or_else(|| {
                method
                    .name
                    .name
                    .eq_ignore_ascii_case("__toString")
                    .then(|| ast::Type {
                        kind: ast::TypeKind::Named(ast::Name {
                            kind: ast::NameKind::Unqualified,
                            text: "string".into(),
                            span: method.name.span,
                        }),
                        span: method.name.span,
                    })
            }),
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
