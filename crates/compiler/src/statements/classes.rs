//! Class declarations compile methods as ordinary functions so calls keep heap-frame execution.

use super::class_default_types::DefaultKind;
use super::property_hooks::HookedDeclaration;
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
        if declaration.name.is_some() && self.class_scope.is_some() {
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
                if declaration.kind != ast::ClassKind::Enum {
                    return Err(self.error("Case can only be used in enums", case_line));
                }
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
                    if let Some(ty) = ty {
                        let member = format!("{display}::{}", constant.name.name);
                        self.validate_literal_member_default(
                            &constant.value,
                            ty,
                            &member,
                            DefaultKind::Constant,
                            self.lines.line(constant.name.span.start),
                        )?;
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
                for property in items {
                    let property_line = self.lines.line(property.span.start);
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
                    if readonly && declaration.name.is_none() && property.default.is_some() {
                        return Err(self.error(
                            format!(
                                "Readonly property {display}::${} cannot have default value",
                                property.name
                            ),
                            self.lines.line(property.span.start),
                        ));
                    }
                    if let (Some(default), Some(ty)) = (&property.default, ty) {
                        let member = format!("{display}::${}", property.name);
                        self.validate_literal_member_default(
                            default,
                            ty,
                            &member,
                            DefaultKind::Property,
                            self.lines.line(property.span.start),
                        )?;
                    }
                    let default = property
                        .default
                        .as_ref()
                        .map(|value| self.compile_initializer(value, &name, trait_scope, line))
                        .transpose()?;
                    let hooks = self.compile_property_metadata(HookedDeclaration {
                        class: &name,
                        trait_scope,
                        kind: declaration.kind,
                        property: &property.name,
                        modifiers,
                        readonly,
                        ty: ty.as_ref(),
                        default: property.default.as_ref(),
                        hooks: &property.hooks,
                        has_hook_list: property.has_hook_list,
                        has_parent: !declaration.extends.is_empty(),
                        line: property_line,
                    })?;
                    let is_final = modifiers.is_final
                        || hooks
                            .as_ref()
                            .is_some_and(|hooks| hooks.set_visibility == Some(Visibility::Private));
                    properties.push(ClassProperty {
                        hooks,
                        name: property.name.clone(),
                        visibility: match modifiers.visibility {
                            Some(ast::Visibility::Private) => Visibility::Private,
                            Some(ast::Visibility::Protected) => Visibility::Protected,
                            _ => Visibility::Public,
                        },
                        is_static: modifiers.is_static,
                        is_final,
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
            self.validate_magic_method(display, method)?;
            for parameter in &method.parameters {
                if parameter.is_promoted() {
                    if !method.name.name.eq_ignore_ascii_case("__construct") {
                        return Err(self.error(
                            "Cannot declare promoted property outside a constructor",
                            self.lines.line(parameter.span.start),
                        ));
                    }
                    if method.body.is_none() {
                        return Err(self.error(
                            "Cannot declare promoted property in an abstract constructor",
                            self.lines.line(parameter.span.start),
                        ));
                    }
                    if (declaration.modifiers.is_readonly || parameter.modifiers.is_readonly)
                        && parameter.ty.is_none()
                    {
                        return Err(self.error(
                            format!(
                                "Readonly property {display}::${} must have type",
                                parameter.name
                            ),
                            self.lines.line(parameter.span.start),
                        ));
                    }
                    let hooks = self.compile_property_metadata(HookedDeclaration {
                        class: &name,
                        trait_scope,
                        kind: declaration.kind,
                        property: &parameter.name,
                        modifiers: &parameter.modifiers,
                        readonly: parameter.modifiers.is_readonly
                            || declaration.modifiers.is_readonly,
                        ty: parameter.ty.as_ref(),
                        default: None,
                        hooks: &parameter.hooks,
                        has_hook_list: parameter.has_hook_list,
                        has_parent: !declaration.extends.is_empty(),
                        line: self.lines.line(parameter.span.start),
                    })?;
                    let is_final = parameter.modifiers.is_final
                        || hooks
                            .as_ref()
                            .is_some_and(|hooks| hooks.set_visibility == Some(Visibility::Private));
                    properties.push(ClassProperty {
                        hooks,
                        name: parameter.name.clone(),
                        visibility: member_visibility(&parameter.modifiers),
                        is_static: false,
                        is_final,
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
            line: line as u32,
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
}

/// All member categories use the same default public visibility.
fn member_visibility(modifiers: &ast::Modifiers) -> Visibility {
    match modifiers.visibility {
        Some(ast::Visibility::Private) => Visibility::Private,
        Some(ast::Visibility::Protected) => Visibility::Protected,
        _ => Visibility::Public,
    }
}
