//! `#[DelayedTargetValidation]` defers errors on built-in attributes until reflection.

use phera_ast::{Attribute, ClassDeclaration, ClassKind, FunctionDeclaration};
use phera_source::attributes::{AttributeTarget, builtin_flags, target_names};

use crate::{CompileError, Compiler, policies::NameKind};

#[derive(Clone, Copy)]
pub(super) enum AttributePlacement {
    Declaration(AttributeTarget),
    Promoted(AttributeTarget),
}

impl Compiler<'_> {
    pub(super) fn validate_attribute_list(
        &self,
        attributes: &[Attribute],
        placement: AttributePlacement,
    ) -> Result<(), CompileError> {
        let (target, promoted) = match placement {
            AttributePlacement::Declaration(target) => (target, None),
            AttributePlacement::Promoted(target) => (
                target,
                Some(match target {
                    AttributeTarget::Property => AttributeTarget::Parameter,
                    _ => AttributeTarget::Property,
                }),
            ),
        };
        let delayed = self.has_builtin(attributes, "DelayedTargetValidation");
        let mut seen = std::collections::HashSet::new();
        for attribute in attributes {
            let name = self.resolve(&attribute.name, NameKind::Class).0;
            let Some(flags) = builtin_flags(&name) else {
                continue;
            };
            let line = self.lines.line(attribute.span.start);
            if flags & target as i64 == 0 {
                if promoted.is_some_and(|other| flags & other as i64 != 0) {
                    continue;
                }
                if !delayed {
                    return Err(self.error(
                        format!(
                            "Attribute \"{name}\" cannot target {} (allowed targets: {})",
                            target.name(),
                            target_names(flags)
                        ),
                        line,
                    ));
                }
            }
            if !seen.insert(name.to_ascii_lowercase()) {
                return Err(self.error(format!("Attribute \"{name}\" must not be repeated"), line));
            }
        }
        Ok(())
    }

    pub(super) fn attribute_includes_target(
        &self,
        attribute: &Attribute,
        placement: AttributePlacement,
    ) -> bool {
        let AttributePlacement::Promoted(target) = placement else {
            return true;
        };
        let name = self.resolve(&attribute.name, NameKind::Class).0;
        builtin_flags(&name).is_none_or(|flags| flags & target as i64 != 0)
    }

    pub(super) fn has_builtin(&self, attributes: &[Attribute], name: &str) -> bool {
        attributes.iter().any(|attribute| {
            self.resolve(&attribute.name, NameKind::Class)
                .0
                .eq_ignore_ascii_case(name)
        })
    }

    pub(super) fn validate_class_attributes(
        &self,
        declaration: &ClassDeclaration,
        name: &str,
    ) -> Result<(), CompileError> {
        let attrs = &declaration.attributes;
        if self.has_builtin(attrs, "DelayedTargetValidation") {
            return Ok(());
        }
        let kind = match declaration.kind {
            ClassKind::Class if declaration.modifiers.is_readonly => "readonly class",
            ClassKind::Class if declaration.modifiers.is_abstract => "abstract class",
            ClassKind::Class => "class",
            ClassKind::Interface => "interface",
            ClassKind::Trait => "trait",
            ClassKind::Enum => "enum",
        };
        for attribute in attrs {
            let resolved = self.resolve(&attribute.name, NameKind::Class).0;
            let invalid = match resolved.to_ascii_lowercase().as_str() {
                "allowdynamicproperties" => !matches!(kind, "class" | "abstract class"),
                "attribute" => !matches!(kind, "class" | "readonly class"),
                "deprecated" => declaration.kind != ClassKind::Trait,
                _ => false,
            };
            if invalid {
                let kind = if resolved.eq_ignore_ascii_case("Deprecated")
                    && declaration.kind == ClassKind::Class
                {
                    "class"
                } else {
                    kind
                };
                return Err(self.error(
                    format!("Cannot apply #[\\{resolved}] to {kind} {name}"),
                    self.lines.line(attribute.span.start),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn validate_nodiscard(
        &self,
        declaration: &FunctionDeclaration,
        line: usize,
    ) -> Result<(), CompileError> {
        if !self.has_builtin(&declaration.attributes, "NoDiscard") {
            return Ok(());
        }
        let is_method =
            declaration.name.name.contains("::") && !declaration.name.name.starts_with("{closure:");
        if is_method
            && let Some((_, method)) = declaration.name.name.rsplit_once("::")
            && matches!(
                method.to_ascii_lowercase().as_str(),
                "__construct" | "__destruct" | "__clone"
            )
        {
            return Err(self.error(
                format!("Method {} cannot be #[\\NoDiscard]", declaration.name.name),
                line,
            ));
        }
        let kind = if is_method { "method" } else { "function" };
        if let Some(declared) = &declaration.return_type {
            let description = match self.declaration_type(declared).as_str() {
                "void" => Some("void"),
                "never" => Some("never returning"),
                _ => None,
            };
            if let Some(description) = description {
                return Err(self.error(format!(
                    "A {description} {kind} does not return a value, but #[\\NoDiscard] requires a return value"
                ), line));
            }
        }
        Ok(())
    }
}
