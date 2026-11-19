//! Invalid linked declarations remain fatal and never enter PHP catch handlers.

use super::{Class, LinkedConstant, class::PropertyDefault};
use phera_bytecode::{ClassDeclaration, ClassId, CompiledUnit, Visibility};
use phera_extension_api::{NativeError, Unwind};
use std::{collections::HashMap, rc::Rc};

/// Missing dependencies are runtime Errors; incompatible declarations are PHP fatal errors.
pub(crate) fn link_failure(unit: &CompiledUnit, index: ClassId, error: NativeError) -> NativeError {
    match error {
        NativeError::Throw {
            class: "Error",
            message,
            ..
        } if !message.ends_with("not found") && !message.ends_with(" - it is not a trait") => {
            NativeError::Unwind(Unwind(Box::new(crate::error_traces::DeclarationFailure {
                message,
                file: unit.file.clone(),
                line: unit.classes[index.index()].line as usize,
            })))
        }
        error => error,
    }
}

/// A signature or visibility error names the method's declaration line.
pub(super) fn method_failure(
    code: &crate::code_table::CodeTable,
    method: &super::Method,
    error: NativeError,
) -> NativeError {
    match (&method.body, error) {
        (super::MethodBody::User(reference), NativeError::Throw { message, .. }) => {
            NativeError::Unwind(Unwind(Box::new(crate::error_traces::DeclarationFailure {
                message,
                file: code.unit(*reference).file.clone(),
                line: code.function(*reference).definition_line as usize,
            })))
        }
        (_, error) => error,
    }
}

/// An explicit class constant resolves interface ambiguity before its initializer runs.
pub(crate) fn merge_interface_constants(
    declaration: &ClassDeclaration,
    unit: &CompiledUnit,
    interfaces: &[Rc<Class>],
    constants: &mut HashMap<String, Rc<LinkedConstant>>,
) -> Result<(), NativeError> {
    for interface in interfaces {
        for (name, constant) in &interface.constants {
            if let Some(previous) = constants.get(name)
                && previous.declaring != constant.declaring
                && !declaration
                    .constants
                    .iter()
                    .any(|own| unit.names[own.name.index()] == *name)
            {
                return Err(NativeError::error(format!(
                    "{} {} inherits both {}::{name} and {}::{name}, which is ambiguous",
                    if declaration.kind == phera_bytecode::ClassKind::Enum {
                        "Enum"
                    } else {
                        "Class"
                    },
                    declaration.name,
                    previous.declaring,
                    constant.declaring
                )));
            }
            constants.insert(name.clone(), constant.clone());
        }
    }
    Ok(())
}

/// Private ancestor properties are separate slots; visible inherited properties cannot narrow access.
pub(crate) fn check_property_visibility(
    class: &str,
    visibility: Visibility,
    name: &str,
    inherited: &[PropertyDefault],
) -> Result<(), NativeError> {
    let Some(inherited) = inherited
        .iter()
        .find(|inherited| inherited.name == name && inherited.visibility != Visibility::Private)
    else {
        return Ok(());
    };
    let narrower = match inherited.visibility {
        Visibility::Public => visibility != Visibility::Public,
        Visibility::Protected => visibility == Visibility::Private,
        Visibility::Private => false,
    };
    if narrower {
        let visibility = if inherited.visibility == Visibility::Public {
            "public"
        } else {
            "protected"
        };
        let suffix = if inherited.visibility == Visibility::Public {
            ""
        } else {
            " or weaker"
        };
        return Err(NativeError::error(format!(
            "Access level to {class}::${name} must be {visibility} (as in class {}){suffix}",
            inherited.declaring
        )));
    }
    Ok(())
}

impl crate::Vm {
    /// Private ancestor properties occupy separate slots. Visible properties inherit a write contract.
    pub(super) fn check_property_override(
        &self,
        property: &PropertyDefault,
        inherited: &PropertyDefault,
        unit: &CompiledUnit,
    ) -> Result<(), NativeError> {
        let class = &property.declaring;
        let name = &property.name;
        let parent = &inherited.declaring;
        if inherited.is_final {
            return Err(NativeError::error(format!(
                "Cannot override final property {parent}::${name}"
            )));
        }
        let inherited_static = inherited.static_storage.is_some();
        if property.static_storage.is_some() != inherited_static {
            let (base, child) = if inherited_static {
                ("static", "non static")
            } else {
                ("non static", "static")
            };
            return Err(NativeError::error(format!(
                "Cannot redeclare {base} {parent}::${name} as {child} {class}::${name}"
            )));
        }
        if property.is_readonly != inherited.is_readonly {
            let (base, child) = if inherited.is_readonly {
                ("readonly", "non-readonly")
            } else {
                ("non-readonly", "readonly")
            };
            return Err(NativeError::error(format!(
                "Cannot redeclare {base} property {parent}::${name} as {child} {class}::${name}"
            )));
        }
        let inherited_set = inherited.set_visibility.unwrap_or(inherited.visibility);
        let narrower_set = matches!(
            (inherited_set, property.set_visibility),
            (
                Visibility::Public,
                Some(Visibility::Protected | Visibility::Private)
            ) | (Visibility::Protected, Some(Visibility::Private))
        );
        if narrower_set {
            let required = if inherited.set_visibility.is_some() {
                format!("protected(set) (as in class {parent}) or weaker")
            } else {
                format!("omitted (as in class {parent})")
            };
            return Err(NativeError::error(format!(
                "Set access level of {class}::${name} must be {required}"
            )));
        }
        let required = match (inherited.visibility, property.visibility) {
            (Visibility::Public, Visibility::Protected | Visibility::Private) => {
                Some(("public", ""))
            }
            (Visibility::Protected, Visibility::Private) => Some(("protected", " or weaker")),
            _ => None,
        };
        if let Some((visibility, suffix)) = required {
            return Err(NativeError::error(format!(
                "Access level to {class}::${name} must be {visibility} (as in class {parent}){suffix}"
            )));
        }
        if property.set_visibility.is_some() || inherited.set_visibility.is_some() {
            let child = property.ty.as_ref().map(|ty| ty.display.as_str());
            let base = inherited.ty.as_ref().map(|ty| ty.display.as_str());
            if child.is_some() != base.is_some()
                || !self.type_subset(child, class, base, parent, unit)
                || !self.type_subset(base, parent, child, class, unit)
            {
                let required = base.map_or_else(
                    || format!("omitted to match the parent definition in class {parent}"),
                    |ty| format!("{ty} (as in class {parent})"),
                );
                return Err(NativeError::error(format!(
                    "Type of {class}::${name} must be {required}"
                )));
            }
        }
        Ok(())
    }

    pub(super) fn magic_override_error(
        &self,
        method: &super::Method,
        error: NativeError,
    ) -> NativeError {
        if !crate::classes::magic::MagicMethods::is_linked_magic(&method.name) {
            return error;
        }
        match (&method.body, error) {
            (
                super::MethodBody::User(function),
                NativeError::Throw {
                    class: "Error",
                    message,
                    ..
                },
            ) => NativeError::Unwind(Unwind(Box::new(crate::error_traces::DeclarationFailure {
                message,
                file: self.code.unit(*function).file.clone(),
                line: self.code.function(*function).definition_line as usize,
            }))),
            (_, error) => error,
        }
    }
}
