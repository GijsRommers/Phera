//! Class declarations compile methods as ordinary functions so calls keep heap-frame execution.

use phera_ast as ast;
use phera_bytecode::{ClassKind, Visibility};

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
    ) -> Result<String, CompileError> {
        if declaration.name.is_some() && self.function.name.contains("::") {
            return Err(self.error("Class declarations may not be nested", line));
        }
        let name = match &declaration.name {
            Some(identifier) if self.namespace.is_empty() => identifier.name.clone(),
            Some(identifier) => format!("{}\\{}", self.namespace, identifier.name),
            None => self.anonymous_class_name(declaration),
        };
        let display = name.split('\0').next().unwrap_or(&name);
        let enum_backing = self.validate_enum(declaration, display)?;
        self.validate_class_declaration(declaration, display)?;
        if declaration.attributes.iter().any(|attribute| {
            attribute.name.text.trim_start_matches('\\') != "AllowDynamicProperties"
        }) {
            return Err(self.error("Attributes not supported yet", line));
        }
        let mut methods = Vec::new();
        let mut properties = Vec::new();
        let mut constants = Vec::new();
        let mut traits = Vec::new();
        let mut enum_cases = Vec::new();
        let trait_scope = (declaration.kind == ast::ClassKind::Trait).then_some(name.as_str());
        for member in &declaration.members {
            if let ast::ClassMemberKind::EnumCase {
                name: case,
                value,
                attributes,
            } = &member.kind
            {
                let case_line = self.lines.line(case.span.start);
                if !attributes.is_empty() {
                    return Err(self.error("Attributes not supported yet", case_line));
                }
                let value = value
                    .as_ref()
                    .map(|value| {
                        self.compile_initializer(
                            value,
                            &name,
                            None,
                            self.lines.line(value.span.start),
                        )
                    })
                    .transpose()?;
                enum_cases.push((case.name.clone(), value));
                continue;
            }
            if let ast::ClassMemberKind::TraitUse {
                traits: names,
                adaptations,
            } = &member.kind
            {
                traits.push(self.compile_trait_use(
                    declaration.kind,
                    display,
                    names,
                    adaptations,
                    member.span,
                )?);
                continue;
            }
            if let ast::ClassMemberKind::Constant {
                modifiers,
                ty,
                items,
                ..
            } = &member.kind
            {
                for constant in items {
                    if constant.name.name.eq_ignore_ascii_case("class") {
                        return Err(self.error("A class constant must not be called 'class'; it is reserved for class name fetching", self.lines.line(member.span.start)));
                    }
                    constants.push(crate::ir::ClassConstant {
                        name: constant.name.name.clone(),
                        visibility: member_visibility(modifiers),
                        is_final: modifiers.is_final,
                        initializer: self.compile_initializer(
                            &constant.value,
                            &name,
                            trait_scope,
                            line,
                        )?,
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
                if !attributes.is_empty() {
                    return Err(self.error("Attributes not supported yet", line));
                }
                if modifiers.set_visibility.is_some() {
                    return Err(self.error("Asymmetric visibility not supported yet", line));
                }
                for property in items {
                    if !property.hooks.is_empty() {
                        return Err(self.error("Property hooks not supported yet", line));
                    }
                    if ty.as_ref().is_some_and(|ty| {
                        self.declaration_type(ty)
                            .split(['|', '&', '?'])
                            .any(|name| name == "callable")
                    }) {
                        return Err(self.error(
                            format!(
                                "Property {display}::${} cannot have type {}",
                                property.name,
                                ty.as_ref()
                                    .map(|ty| self.declaration_type(ty))
                                    .unwrap_or_default()
                            ),
                            self.lines.line(property.span.start),
                        ));
                    }
                    let readonly = modifiers.is_readonly || declaration.modifiers.is_readonly;
                    if readonly && modifiers.is_static {
                        return Err(self.error(
                            format!(
                                "Static property {display}::${} cannot be readonly",
                                property.name
                            ),
                            self.lines.line(property.span.start),
                        ));
                    }
                    if readonly && ty.is_none() {
                        return Err(self.error(
                            format!(
                                "Readonly property {display}::${} must have type",
                                property.name
                            ),
                            self.lines.line(property.span.start),
                        ));
                    }
                    let default = property
                        .default
                        .as_ref()
                        .map(|value| self.compile_initializer(value, &name, trait_scope, line))
                        .transpose()?;
                    properties.push(ClassProperty {
                        name: property.name.clone(),
                        visibility: match modifiers.visibility {
                            Some(ast::Visibility::Private) => Visibility::Private,
                            Some(ast::Visibility::Protected) => Visibility::Protected,
                            _ => Visibility::Public,
                        },
                        is_static: modifiers.is_static,
                        is_readonly: modifiers.is_readonly || declaration.modifiers.is_readonly,
                        ty: ty
                            .as_ref()
                            .map(|declared_type| self.declaration_type(declared_type)),
                        default,
                    });
                }
                continue;
            }
            let ast::ClassMemberKind::Method(method) = &member.kind else {
                return Err(self.error("Class properties and constants not supported yet", line));
            };
            if methods
                .iter()
                .any(|entry: &ClassMethod| entry.name.eq_ignore_ascii_case(&method.name.name))
            {
                return Err(self.error(
                    format!("Cannot redeclare {display}::{}()", method.name.name),
                    self.lines.line(method.name.span.start),
                ));
            }
            if method.name.name.eq_ignore_ascii_case("__invoke")
                && matches!(
                    method.modifiers.visibility,
                    Some(ast::Visibility::Private | ast::Visibility::Protected)
                )
            {
                self.declaration_warnings
                    .push(phera_bytecode::DeclarationWarning {
                        message: format!(
                            "The magic method {name}::{}() must have public visibility",
                            method.name.name
                        ),
                        line: self.lines.line(method.name.span.start) as u32,
                    });
            }
            for parameter in &method.parameters {
                if parameter.modifiers.visibility.is_some() {
                    properties.push(ClassProperty {
                        name: parameter.name.clone(),
                        visibility: member_visibility(&parameter.modifiers),
                        is_static: false,
                        is_readonly: parameter.modifiers.is_readonly
                            || declaration.modifiers.is_readonly,
                        ty: parameter
                            .ty
                            .as_ref()
                            .map(|declared_type| self.declaration_type(declared_type)),
                        default: None,
                    });
                }
            }
            methods.push(self.compile_method(
                &name,
                trait_scope,
                method,
                self.lines.line(method.name.span.start),
            )?);
        }
        let can_hoist = timing == DeclarationTiming::Hoisted
            && declaration.kind != ast::ClassKind::Enum
            && traits.is_empty();
        let index = self.classes.len();
        self.classes.push(ClassDeclaration {
            name: name.clone(),
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
            kind: match (declaration.kind, &declaration.name) {
                (ast::ClassKind::Interface, _) => ClassKind::Interface,
                (ast::ClassKind::Trait, _) => ClassKind::Trait,
                (ast::ClassKind::Enum, _) => ClassKind::Enum,
                (_, None) => ClassKind::Anonymous,
                _ => ClassKind::Class,
            },
            is_abstract: declaration.modifiers.is_abstract,
            is_final: declaration.modifiers.is_final || declaration.kind == ast::ClassKind::Enum,
            is_readonly: declaration.modifiers.is_readonly,
            allows_dynamic_properties: declaration
                .attributes
                .iter()
                .any(|attribute| attribute.name.text == "AllowDynamicProperties"),
            constants,
            properties,
            methods,
            traits,
            enum_backing,
            enum_cases,
        });
        if can_hoist {
            self.hoisted_classes.push(index);
        }
        self.emit(Instruction::DeclareClass(index), line);
        Ok(name)
    }

    /// The first named ancestor supplies the prefix of PHP's anonymous class name.
    fn anonymous_class_name(&self, declaration: &ast::ClassDeclaration) -> String {
        let prefix = declaration
            .extends
            .first()
            .or_else(|| declaration.implements.first())
            .map(|ancestor| self.resolve(ancestor, NameKind::Class).0)
            .unwrap_or_else(|| "class".into());
        let number = self.anonymous_classes.get();
        self.anonymous_classes.set(number + 1);
        let line = self.lines.line(declaration.span.start);
        format!("{prefix}@anonymous\0{}:{line}${number:x}", self.file)
    }

    /// A separate pure function retains expression evaluation order without storing AST nodes.
    fn compile_initializer(
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
    fn compile_method(
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

/// All member categories use the same default public visibility.
fn member_visibility(modifiers: &ast::Modifiers) -> Visibility {
    match modifiers.visibility {
        Some(ast::Visibility::Private) => Visibility::Private,
        Some(ast::Visibility::Protected) => Visibility::Protected,
        _ => Visibility::Public,
    }
}
