//! Invalid linked declarations remain fatal and never enter PHP catch handlers.

use super::{Class, LinkedConstant, class::PropertyDefault};
use phera_bytecode::{
    ClassDeclaration, ClassId, CompiledUnit, Instruction, PropertyDeclaration, Visibility,
};
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
            let line = std::iter::once(&unit.main).chain(&unit.functions).find_map(|function| {
                function.instructions.iter().position(|instruction| matches!(instruction, Instruction::DeclareClass { class } if *class == index))
                    .and_then(|position| function.lines.get(position).copied())
            }).unwrap_or(1);
            NativeError::Unwind(Unwind(Box::new(crate::error_traces::DeclarationFailure {
                message,
                file: unit.file.clone(),
                line: line as usize,
            })))
        }
        error => error,
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
