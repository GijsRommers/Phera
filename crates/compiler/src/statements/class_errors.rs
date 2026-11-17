//! Declaration diagnostics remain fatal rather than becoming catchable runtime Errors.

use crate::{CompileError, Compiler};
use phera_ast as ast;

impl Compiler<'_> {
    /// Reject structurally invalid classes before any top-level instruction runs.
    pub(crate) fn validate_class_declaration(
        &self,
        declaration: &ast::ClassDeclaration,
        name: &str,
    ) -> Result<(), CompileError> {
        let is_interface = declaration.kind == ast::ClassKind::Interface;
        if let Some(identifier) = &declaration.name
            && matches!(
                identifier.name.to_ascii_lowercase().as_str(),
                "self" | "parent" | "static"
            )
        {
            let message = if is_interface {
                format!(
                    "Cannot use \"{}\" as interface name, as it is reserved",
                    identifier.name
                )
            } else {
                format!(
                    "Cannot use \"{}\" as a class name as it is reserved",
                    identifier.name
                )
            };
            return Err(self.error(message, self.lines.line(identifier.span.start)));
        }
        for parent in declaration.extends.iter().chain(&declaration.implements) {
            if matches!(
                parent.text.to_ascii_lowercase().as_str(),
                "self" | "parent" | "static"
            ) {
                let kind = if is_interface || declaration.implements.contains(parent) {
                    "interface"
                } else {
                    "class"
                };
                return Err(self.error(
                    format!(
                        "Cannot use \"{}\" as {kind} name, as it is reserved",
                        parent.text
                    ),
                    self.lines.line(parent.span.start),
                ));
            }
        }
        let mut properties = std::collections::HashSet::new();
        for member in &declaration.members {
            let line = self.lines.line(member.span.start);
            match &member.kind {
                ast::ClassMemberKind::Method(method) => {
                    self.validate_method_declaration(declaration, name, method)?
                }
                ast::ClassMemberKind::Property {
                    modifiers, items, ..
                } => {
                    for property in items {
                        if !properties.insert(&property.name) {
                            return Err(self.error(
                                format!("Cannot redeclare {name}::${}", property.name),
                                self.lines.line(property.span.start),
                            ));
                        }
                        if modifiers.is_abstract && property.hooks.is_empty() {
                            return Err(
                                self.error("Only hooked properties may be declared abstract", line)
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Missing bodies and abstract declarations have compile-time PHP diagnostics.
    fn validate_method_declaration(
        &self,
        declaration: &ast::ClassDeclaration,
        class: &str,
        method: &ast::Method,
    ) -> Result<(), CompileError> {
        let line = self.lines.line(method.name.span.start);
        let name = &method.name.name;
        let is_interface = declaration.kind == ast::ClassKind::Interface;
        let message = if name.eq_ignore_ascii_case("__construct") && method.modifiers.is_static {
            Some(format!("Method {class}::{name}() cannot be static"))
        } else if !is_interface && method.body.is_none() && !method.modifiers.is_abstract {
            Some(format!(
                "Non-abstract method {class}::{name}() must contain body"
            ))
        } else if method.modifiers.is_abstract
            && method.modifiers.visibility == Some(ast::Visibility::Private)
        {
            Some(format!(
                "Abstract function {class}::{name}() cannot be declared private"
            ))
        } else if method.modifiers.is_abstract
            && !is_interface
            && !declaration.modifiers.is_abstract
        {
            Some(format!(
                "Class {class} declares abstract method {name}() and must therefore be declared abstract"
            ))
        } else {
            None
        };
        if let Some(message) = message {
            return Err(self.error(message, line));
        }
        Ok(())
    }
}
