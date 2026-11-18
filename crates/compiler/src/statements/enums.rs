//! `enum Status: int { case Ready = 1; }` checks structural restrictions before execution.

use std::collections::HashSet;

use phera_ast::{ClassDeclaration, ClassKind, ClassMemberKind};
use phera_bytecode::EnumBacking;

use crate::{CompileError, Compiler};

impl Compiler<'_> {
    pub(crate) fn validate_enum(
        &self,
        declaration: &ClassDeclaration,
        name: &str,
    ) -> Result<Option<EnumBacking>, CompileError> {
        if declaration.kind != ClassKind::Enum {
            return Ok(None);
        }
        let backing = declaration
            .backing_type
            .as_ref()
            .map(|ty| {
                let display = self.declaration_type(ty);
                match display.as_str() {
                    "int" => Ok(EnumBacking::Int),
                    "string" => Ok(EnumBacking::String),
                    _ => Err(self.error(
                        format!(
                            "Enum backing type must be int or string, {} given",
                            backing_type_display(&display)
                        ),
                        self.lines.line(declaration.span.start),
                    )),
                }
            })
            .transpose()?;
        let mut constants = HashSet::new();
        for member in &declaration.members {
            let line = self.lines.line(member.span.start);
            match &member.kind {
                ClassMemberKind::Property { .. } => {
                    return Err(self.error(format!("Enum {name} cannot include properties"), line));
                }
                ClassMemberKind::Method(method) if method.modifiers.is_abstract => {
                    let method = &method.name.name;
                    return Err(self.error(
                        format!("Enum method {name}::{method}() must not be abstract"),
                        line,
                    ));
                }
                ClassMemberKind::Method(method)
                    if let Some(promoted) = method
                        .parameters
                        .iter()
                        .find(|parameter| parameter.modifiers.visibility.is_some()) =>
                {
                    return Err(self.error(
                        format!("Enum {name} cannot include properties"),
                        self.lines.line(promoted.span.start),
                    ));
                }
                ClassMemberKind::EnumCase {
                    name: case, value, ..
                } => {
                    let case = &case.name;
                    if case.eq_ignore_ascii_case("class") {
                        return Err(self.error("A class constant must not be called 'class'; it is reserved for class name fetching", line));
                    }
                    if !constants.insert(case) {
                        return Err(self.error(
                            format!("Cannot redefine class constant {name}::{case}"),
                            line,
                        ));
                    }
                    match (backing, value) {
                        (Some(_), None) => {
                            return Err(self.error(
                                format!("Case {case} of backed enum {name} must have a value"),
                                line,
                            ));
                        }
                        (None, Some(_)) => {
                            return Err(self.error(
                                format!(
                                    "Case {case} of non-backed enum {name} must not have a value"
                                ),
                                line,
                            ));
                        }
                        _ => {}
                    }
                }
                ClassMemberKind::Constant { items, .. } => {
                    for constant in items {
                        let constant = &constant.name.name;
                        if !constants.insert(constant) {
                            return Err(self.error(
                                format!("Cannot redefine class constant {name}::{constant}"),
                                line,
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(backing)
    }
}

/// PHP prints named types first, then built-in types in mask order.
fn backing_type_display(display: &str) -> String {
    const TYPE_ORDER: &[&str] = &[
        "static", "callable", "object", "array", "string", "int", "float", "bool", "false", "true",
        "void", "never", "null",
    ];
    let mut members: Vec<_> = display.split('|').collect();
    members.sort_by_key(|member| {
        TYPE_ORDER
            .iter()
            .position(|ty| ty == member)
            .map_or(0, |index| index + 1)
    });
    members.join("|")
}
