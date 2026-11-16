//! Class declarations compile methods as ordinary functions so calls keep heap-frame execution.

use phera_ast as ast;
use phera_bytecode::{ClassKind, Visibility};

use crate::ir::{ClassDeclaration, ClassMethod, Instruction};
use crate::policies::{DeclarationTiming, NameKind};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Preserve declarations separately from global functions and reject unimplemented members.
    pub(crate) fn compile_class(
        &mut self,
        declaration: &ast::ClassDeclaration,
        timing: DeclarationTiming,
        line: usize,
    ) -> Result<(), CompileError> {
        if !matches!(
            declaration.kind,
            ast::ClassKind::Class | ast::ClassKind::Interface
        ) {
            return Err(self.error("Traits and enums not supported yet", line));
        }
        let Some(identifier) = &declaration.name else {
            return Err(self.error("Anonymous classes not supported yet", line));
        };
        if declaration.modifiers.is_readonly {
            return Err(self.error("Readonly classes not supported yet", line));
        }
        let name = if self.namespace.is_empty() {
            identifier.name.clone()
        } else {
            format!("{}\\{}", self.namespace, identifier.name)
        };
        let mut methods = Vec::new();
        for member in &declaration.members {
            let ast::ClassMemberKind::Method(method) = &member.kind else {
                return Err(self.error("Class properties and constants not supported yet", line));
            };
            if methods
                .iter()
                .any(|entry: &ClassMethod| entry.name.eq_ignore_ascii_case(&method.name.name))
            {
                return Err(self.error(
                    format!("Cannot redeclare {name}::{}()", method.name.name),
                    line,
                ));
            }
            methods.push(self.compile_method(&name, method, line)?);
        }
        let index = self.classes.len();
        self.classes.push(ClassDeclaration {
            name,
            parent: declaration
                .extends
                .first()
                .filter(|_| declaration.kind == ast::ClassKind::Class)
                .map(|parent| self.resolve(parent, NameKind::Class).0),
            interfaces: declaration
                .implements
                .iter()
                .chain(declaration.extends.iter().skip(
                    if declaration.kind == ast::ClassKind::Interface {
                        0
                    } else {
                        1
                    },
                ))
                .map(|interface| self.resolve(interface, NameKind::Class).0)
                .collect(),
            kind: if declaration.kind == ast::ClassKind::Interface {
                ClassKind::Interface
            } else {
                ClassKind::Class
            },
            is_abstract: declaration.modifiers.is_abstract,
            is_final: declaration.modifiers.is_final,
            methods,
        });
        if timing == DeclarationTiming::Hoisted {
            self.hoisted_classes.push(index);
        }
        self.emit(Instruction::DeclareClass(index), line);
        Ok(())
    }

    /// Reuse parameter/default lowering while keeping method names out of the function table.
    fn compile_method(
        &mut self,
        class: &str,
        method: &ast::Method,
        line: usize,
    ) -> Result<ClassMethod, CompileError> {
        if method
            .parameters
            .iter()
            .any(|parameter| parameter.modifiers != ast::Modifiers::default())
        {
            return Err(self.error("Constructor promotion not supported yet", line));
        }
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
        let namespace = std::mem::take(&mut self.namespace);
        let result = self.compile_function(&declaration, DeclarationTiming::Hoisted, line);
        self.namespace = namespace;
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
