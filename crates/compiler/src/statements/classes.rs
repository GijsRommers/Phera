//! Class declarations compile methods as ordinary functions so calls keep heap-frame execution.

use super::builtin_attributes::AttributePlacement;
use super::class_default_types::DefaultKind;
use super::property_hooks::HookedDeclaration;
use phera_ast as ast;
use phera_bytecode::{ClassKind, Visibility};
use phera_source::attributes::AttributeTarget;

use crate::ir::{ClassDeclaration, ClassProperty, Instruction};
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
        let mut metadata = self.declaration_metadata(
            &declaration.attributes,
            declaration.span,
            declaration.doc_comment,
            Some(&name),
            AttributePlacement::Declaration(AttributeTarget::Class),
        )?;
        if !metadata.attributes.is_empty() {
            self.validate_class_attributes(declaration, display, &mut metadata)?;
        }
        let mut methods = Vec::new();
        // Lowercased, to find a redeclared method without comparing it with
        // every earlier one: a generated class can have 100,000 methods.
        let mut method_names = std::collections::HashSet::new();
        let mut properties = Vec::new();
        let mut constants = Vec::new();
        let mut traits = Vec::new();
        let mut enum_cases = Vec::new();
        let trait_scope = (declaration.kind == ast::ClassKind::Trait).then_some(name.as_str());
        // The resolved parent, known before the members compile, so a
        // `parent::CONST` in a method can use the parent's own name.
        let parent = declaration
            .extends
            .first()
            .filter(|_| declaration.kind == ast::ClassKind::Class)
            .map(|parent| self.resolve(parent, NameKind::Class).0);
        let previous_parent = std::mem::replace(&mut self.parent_scope, parent.clone());
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
                let metadata = self.declaration_metadata(
                    attributes,
                    member.span,
                    member.doc_comment,
                    Some(&name),
                    AttributePlacement::Declaration(AttributeTarget::ClassConstant),
                )?;
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
                enum_cases.push((case.name.clone(), value, metadata));
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
                self.reject_member_set_visibility(modifiers, "class constant", member.span)?;
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
                        metadata: metadata.clone(),
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
                let metadata = self.declaration_metadata(
                    attributes,
                    member.span,
                    member.doc_comment,
                    Some(&name),
                    AttributePlacement::Declaration(AttributeTarget::Property),
                )?;
                for property in items {
                    let property_line = self.lines.line(property.span.start);
                    let readonly = modifiers.is_readonly || declaration.modifiers.is_readonly;
                    let declared_type = ty.as_ref().map(|ty| self.declaration_type(ty));
                    let (set_visibility, is_final) = self.property_write_visibility(
                        modifiers,
                        (display, &property.name, property.span),
                        declared_type.as_deref(),
                        readonly,
                    )?;
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
                        self.validate_literal_member_default(
                            default,
                            ty,
                            &format!("{display}::${}", property.name),
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
                    properties.push(ClassProperty {
                        hooks,
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
                        ty: declared_type,
                        default,
                    });
                }
                continue;
            }
            let ast::ClassMemberKind::Method(method) = &member.kind else {
                return Err(self.error("Class properties and constants not supported yet", line));
            };
            self.reject_member_set_visibility(&method.modifiers, "method", method.span)?;
            if !method_names.insert(method.name.name.to_ascii_lowercase()) {
                return Err(self.error(
                    format!("Cannot redeclare {display}::{}()", method.name.name),
                    self.lines.line(method.name.span.start),
                ));
            }
            self.validate_magic_method(display, method)?;
            for parameter in &method.parameters {
                let parameter_line = || self.lines.line(parameter.span.start);
                if parameter.modifiers.is_static {
                    return Err(self.error(
                        "Cannot use the static modifier on a parameter",
                        parameter_line(),
                    ));
                }
                if parameter.is_promoted() {
                    if !method.name.name.eq_ignore_ascii_case("__construct") {
                        return Err(self.error(
                            "Cannot declare promoted property outside a constructor",
                            parameter_line(),
                        ));
                    }
                    if method.body.is_none() {
                        return Err(self.error(
                            "Cannot declare promoted property in an abstract constructor",
                            parameter_line(),
                        ));
                    }
                    if parameter.variadic {
                        return Err(self.error(
                            "Cannot declare variadic promoted property",
                            parameter_line(),
                        ));
                    }
                    let metadata = self.declaration_metadata(
                        &parameter.attributes,
                        parameter.span,
                        parameter.doc_comment,
                        Some(&name),
                        AttributePlacement::Promoted(AttributeTarget::Property),
                    )?;
                    let readonly =
                        parameter.modifiers.is_readonly || declaration.modifiers.is_readonly;
                    let ty = parameter.ty.as_ref().map(|ty| self.declaration_type(ty));
                    let (set_visibility, is_final) = self.property_write_visibility(
                        &parameter.modifiers,
                        (display, &parameter.name, parameter.span),
                        ty.as_deref(),
                        readonly,
                    )?;
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
                    properties.push(ClassProperty {
                        hooks,
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
                trait_scope,
                method,
                member.doc_comment,
                self.lines.line(method.name.span.start),
            )?);
        }
        self.parent_scope = previous_parent;
        let can_hoist = timing == DeclarationTiming::Hoisted
            && declaration.kind != ast::ClassKind::Enum
            && traits.is_empty();
        let index = self.classes.len();
        let allows_dynamic_properties = metadata.attributes.iter().any(|attribute| {
            attribute
                .name
                .eq_ignore_ascii_case("AllowDynamicProperties")
        });
        self.classes.push(ClassDeclaration {
            metadata,
            line: line as u32,
            name: name.clone(),
            parent,
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
            allows_dynamic_properties,
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
