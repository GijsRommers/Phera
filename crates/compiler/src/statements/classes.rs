//! Class declarations compile methods as ordinary functions so calls keep heap-frame execution.

use super::builtin_attributes::AttributePlacement;
use super::class_default_types::DefaultKind;
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
        self.validate_class_attributes(declaration, display, &mut metadata)?;
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
                    if !property.hooks.is_empty() {
                        return Err(self.error("Property hooks not supported yet", line));
                    }
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
                        ty: declared_type,
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
                    format!("Cannot redeclare {display}::{}()", method.name.name),
                    self.lines.line(method.name.span.start),
                ));
            }
            self.validate_magic_method(display, method)?;
            for parameter in &method.parameters {
                let parameter_line = self.lines.line(parameter.span.start);
                if parameter.modifiers.is_static {
                    return Err(self.error(
                        "Cannot use the static modifier on a parameter",
                        parameter_line,
                    ));
                }
                if parameter.is_promoted() {
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
                        (display, &parameter.name, parameter.span),
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
                trait_scope,
                method,
                member.doc_comment,
                self.lines.line(method.name.span.start),
            )?);
        }
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
