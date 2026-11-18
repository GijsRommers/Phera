//! Invalid linked declarations remain fatal and never enter PHP catch handlers.

use super::{Class, LinkedConstant, class::PropertyDefault};
use phera_bytecode::{ClassDeclaration, ClassId, CompiledUnit, PropertyDeclaration, Visibility};
use phera_extension_api::{NativeError, Unwind};
use std::{collections::HashMap, rc::Rc};

/// Missing dependencies are runtime Errors; incompatible declarations are PHP fatal errors.
pub(crate) fn link_failure(unit: &CompiledUnit, index: ClassId, error: NativeError) -> NativeError {
    match error {
        NativeError::Throw {
            class: "Error",
            message,
            ..
        } if !message.ends_with("not found") => {
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
                    "Class {} inherits both {}::{name} and {}::{name}, which is ambiguous",
                    declaration.name, previous.declaring, constant.declaring
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
    property: &PropertyDeclaration,
    name: &str,
    inherited: &[PropertyDefault],
) -> Result<(), NativeError> {
    let Some(inherited) = inherited
        .iter()
        .find(|inherited| inherited.name == name && inherited.visibility != Visibility::Private)
    else {
        return Ok(());
    };
    if inherited.is_final {
        return Err(NativeError::error(format!(
            "Cannot override final property {}::${name}",
            inherited.declaring
        )));
    }
    let inherited_set = inherited.set_visibility.unwrap_or(inherited.visibility);
    let own_set = property.set_visibility.unwrap_or(property.visibility);
    if matches!(
        (inherited_set, own_set),
        (Visibility::Protected, Visibility::Private)
    ) && inherited.set_visibility.is_some()
    {
        return Err(NativeError::error(format!(
            "Set access level of {class}::${name} must be protected(set) (as in class {}) or weaker",
            inherited.declaring
        )));
    }
    let narrower = match inherited.visibility {
        Visibility::Public => property.visibility != Visibility::Public,
        Visibility::Protected => property.visibility == Visibility::Private,
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
