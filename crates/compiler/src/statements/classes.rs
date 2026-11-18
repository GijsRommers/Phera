//! Class declarations compile methods as ordinary functions so calls keep heap-frame execution.

use super::builtin_attributes::AttributePlacement;
use phera_ast as ast;
use phera_bytecode::{ClassKind, Visibility};
use phera_source::attributes::AttributeTarget;

use crate::ir::{ClassDeclaration, ClassMethod, ClassProperty, Instruction};
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
        if self.class_scope.is_some() {
            return Err(self.error("Class declarations may not be nested", line));
        }
        if !matches!(
            declaration.kind,
            ast::ClassKind::Class | ast::ClassKind::Interface
        ) {
            return Err(self.error("Traits and enums not supported yet", line));
        }
        let Some(identifier) = &declaration.name else {
            return Err(self.error("Anonymous classes not supported yet", line));
        };
        let name = if self.namespace.is_empty() {
            identifier.name.clone()
        } else {
            format!("{}\\{}", self.namespace, identifier.name)
        };
        self.validate_class_declaration(declaration, &name)?;
        let mut metadata = self.declaration_metadata(
            &declaration.attributes,
            declaration.span,
            declaration.doc_comment,
            Some(&name),
            AttributePlacement::Declaration(AttributeTarget::Class),
        )?;
        self.validate_class_attributes(declaration, &name, &mut metadata)?;
        let mut methods = Vec::new();
        let mut properties = Vec::new();
        let mut constants = Vec::new();
        for member in &declaration.members {
            if let ast::ClassMemberKind::Constant {
                modifiers,
                ty,
                items,
                attributes,
            } = &member.kind
            {
                let metadata = self.declaration_metadata(
                    attributes,
                    member.span,
                    member.doc_comment,
                    Some(&name),
                    AttributePlacement::Declaration(AttributeTarget::ClassConstant),
                )?;
                self.reject_member_set_visibility(
                    modifiers,
                    "class constant",
                    self.lines.line(member.span.start),
                )?;
                for constant in items {
                    if constant.name.name.eq_ignore_ascii_case("class") {
                        return Err(self.error("A class constant must not be called 'class'; it is reserved for class name fetching", self.lines.line(member.span.start)));
                    }
                    constants.push(crate::ir::ClassConstant {
                        metadata: metadata.clone(),
                        name: constant.name.name.clone(),
                        visibility: member_visibility(modifiers),
                        is_final: modifiers.is_final,
                        initializer: self.compile_initializer(&constant.value, line)?,
                        ty: ty
                            .as_ref()
                            .map(|declared_type| self.declaration_type(declared_type)),
                    });
                }
                continue;
            }
            if let ast::ClassMemberKind::Property {
                attributes,
                modifiers,
                ty,
                items,
            } = &member.kind
            {
                let metadata = self.declaration_metadata(
                    attributes,
                    member.span,
                    member.doc_comment,
                    Some(&name),
                    AttributePlacement::Declaration(AttributeTarget::Property),
                )?;
                for property in items {
                    if !property.hooks.is_empty() {
                        return Err(self.error("Property hooks not supported yet", line));
                    }
                    let readonly = modifiers.is_readonly || declaration.modifiers.is_readonly;
                    let ty = ty.as_ref().map(|ty| self.declaration_type(ty));
                    let (set_visibility, is_final) = self.property_write_visibility(
                        modifiers,
                        (&name, &property.name, property.span),
                        ty.as_deref(),
                        readonly,
                    )?;
                    let default = property
                        .default
                        .as_ref()
                        .map(|value| self.compile_initializer(value, line))
                        .transpose()?;
                    properties.push(ClassProperty {
                        name: property.name.clone(),
                        set_visibility,
                        is_final,
                        metadata: metadata.clone(),
                        promoted: false,
                        visibility: match modifiers.visibility {
                            Some(ast::Visibility::Private) => Visibility::Private,
                            Some(ast::Visibility::Protected) => Visibility::Protected,
                            _ => Visibility::Public,
                        },
                        is_static: modifiers.is_static,
                        is_readonly: modifiers.is_readonly || declaration.modifiers.is_readonly,
                        ty,
                        default,
                    });
                }
                continue;
            }
            let ast::ClassMemberKind::Method(method) = &member.kind else {
                return Err(self.error("Class properties and constants not supported yet", line));
            };
            self.reject_member_set_visibility(
                &method.modifiers,
                "method",
                self.lines.line(method.span.start),
            )?;
            if methods
                .iter()
                .any(|entry: &ClassMethod| entry.name.eq_ignore_ascii_case(&method.name.name))
            {
                return Err(self.error(
                    format!("Cannot redeclare {name}::{}()", method.name.name),
                    self.lines.line(method.name.span.start),
                ));
            }
            self.validate_magic_method(&name, method)?;
            for parameter in &method.parameters {
                let parameter_line = self.lines.line(parameter.span.start);
                if parameter.modifiers.is_static {
                    return Err(self.error(
                        "Cannot use the static modifier on a parameter",
                        parameter_line,
                    ));
                }
                if parameter.modifiers.promotes_property() {
                    if !method.name.name.eq_ignore_ascii_case("__construct") {
                        return Err(self.error(
                            "Cannot declare promoted property outside a constructor",
                            parameter_line,
                        ));
                    }
                    if method.body.is_none() {
                        return Err(self.error(
                            "Cannot declare promoted property in an abstract constructor",
                            parameter_line,
                        ));
                    }
                    if parameter.variadic {
                        return Err(
                            self.error("Cannot declare variadic promoted property", parameter_line)
                        );
                    }
                    let metadata = self.declaration_metadata(
                        &parameter.attributes,
                        parameter.span,
                        None,
                        Some(&name),
                        AttributePlacement::Promoted(AttributeTarget::Property),
                    )?;
                    let readonly =
                        parameter.modifiers.is_readonly || declaration.modifiers.is_readonly;
                    let ty = parameter.ty.as_ref().map(|ty| self.declaration_type(ty));
                    let (set_visibility, is_final) = self.property_write_visibility(
                        &parameter.modifiers,
                        (&name, &parameter.name, parameter.span),
                        ty.as_deref(),
                        readonly,
                    )?;
                    properties.push(ClassProperty {
                        name: parameter.name.clone(),
                        set_visibility,
                        is_final,
                        metadata,
                        promoted: true,
                        visibility: member_visibility(&parameter.modifiers),
                        is_static: false,
                        is_readonly: parameter.modifiers.is_readonly
                            || declaration.modifiers.is_readonly,
                        ty,
                        default: None,
                    });
                }
            }
            methods.push(self.compile_method(
                &name,
                method,
                member.doc_comment,
                self.lines.line(method.name.span.start),
            )?);
        }
        let index = self.classes.len();
        let allows_dynamic_properties = metadata.attributes.iter().any(|attribute| {
            attribute
                .name
                .eq_ignore_ascii_case("AllowDynamicProperties")
        });
        self.classes.push(ClassDeclaration {
            metadata,
            line: line as u32,
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
            is_readonly: declaration.modifiers.is_readonly,
            allows_dynamic_properties,
            constants,
            properties,
            methods,
        });
        if timing == DeclarationTiming::Hoisted {
            self.hoisted_classes.push(index);
        }
        self.emit(Instruction::DeclareClass(index), line);
        Ok(())
    }

    /// A separate pure function retains expression evaluation order without storing AST nodes.
    fn compile_initializer(
        &mut self,
        expression: &ast::Expression,
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
            self.reflection_initializers,
        );
        builder.imports = self.imports.clone();
        let result = builder.compile_expression(expression)?;
        builder.emit(Instruction::Return(result), line);
        let index = self.functions.len();
        self.append_function(builder, DeclarationTiming::Hidden, line);
        Ok(index)
    }

    /// Reuse parameter/default lowering while keeping method names out of the function table.
    fn compile_method(
        &mut self,
        class: &str,
        method: &ast::Method,
        doc_comment: Option<phera_source::Span>,
        line: usize,
    ) -> Result<ClassMethod, CompileError> {
        use super::functions::{FunctionBody, FunctionSource};
        let name = format!("{class}::{}", method.name.name);
        let implicit_return = (method.return_type.is_none()
            && method.name.name.eq_ignore_ascii_case("__toString"))
        .then(|| ast::Type {
            kind: ast::TypeKind::Named(ast::Name {
                kind: ast::NameKind::Unqualified,
                text: "string".into(),
                span: method.name.span,
            }),
            span: method.name.span,
        });
        let declaration = FunctionSource {
            span: method.span,
            doc_comment,
            attributes: &method.attributes,
            name: &name,
            by_reference: method.by_reference,
            parameters: &method.parameters,
            return_type: method.return_type.as_ref().or(implicit_return.as_ref()),
            body: FunctionBody::Statements(method.body.as_deref().unwrap_or_default()),
        };
        let index = self.functions.len();
        self.compile_captured_function(&declaration, DeclarationTiming::Hidden, line, &[])?;
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

/// All member categories use the same default public visibility.
fn member_visibility(modifiers: &ast::Modifiers) -> Visibility {
    match modifiers.visibility {
        Some(ast::Visibility::Private) => Visibility::Private,
        Some(ast::Visibility::Protected) => Visibility::Protected,
        _ => Visibility::Public,
    }
}
