//! Structural method checks run before publishing inherited tables, avoiding partially linked classes.

use std::{collections::HashMap, rc::Rc};

use phera_bytecode::{ClassKind, Visibility};
use phera_extension_api::NativeError;

use super::{Class, Method};

/// Private methods occupy independent scopes; public/protected overrides must preserve contracts.
pub(crate) fn check_override(
    class: &str,
    inherited: &Method,
    method: &Method,
) -> Result<(), NativeError> {
    if inherited.is_final {
        return Err(NativeError::error(format!(
            "Cannot override final method {}::{}()",
            inherited.declaring, inherited.name
        )));
    }
    if inherited.visibility == Visibility::Private {
        return Ok(());
    }
    if inherited.is_static != method.is_static {
        let (before, after) = if inherited.is_static {
            ("static", "non static")
        } else {
            ("non static", "static")
        };
        return Err(NativeError::error(format!(
            "Cannot make {before} method {}::{}() {after} in class {class}",
            inherited.declaring, inherited.name
        )));
    }
    let narrower = match inherited.visibility {
        Visibility::Public => method.visibility != Visibility::Public,
        Visibility::Protected => method.visibility == Visibility::Private,
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
            "Access level to {class}::{}() must be {visibility}{suffix} (as in class {})",
            method.name, inherited.declaring
        )));
    }
    Ok(())
}

/// Concrete classes must implement abstract ancestor and interface methods.
pub(crate) fn check_abstract(
    class: &str,
    kind: ClassKind,
    is_abstract: bool,
    methods: &HashMap<String, Rc<Method>>,
    interfaces: &[Rc<Class>],
) -> Result<(), NativeError> {
    if is_abstract || kind == ClassKind::Interface {
        return Ok(());
    }
    let mut missing: Vec<_> = methods
        .values()
        .filter(|method| method.is_abstract)
        .map(|method| format!("{}::{}", method.declaring, method.name))
        .collect();
    for interface in interfaces {
        for (name, method) in &interface.methods {
            if methods.get(name).is_none_or(|method| method.is_abstract) {
                missing.push(format!("{}::{}", method.declaring, method.name));
            }
        }
    }
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        return Ok(());
    }
    let count = missing.len();
    let plural = if count == 1 { "" } else { "s" };
    if kind == ClassKind::Anonymous {
        let class = class.split('\0').next().unwrap_or(class);
        return Err(NativeError::error(format!(
            "Class {class} must implement {count} abstract method{plural} ({})",
            missing.join(", ")
        )));
    }
    Err(NativeError::error(format!(
        "Class {class} contains {count} abstract method{plural} and must therefore be declared abstract or implement the remaining methods ({})",
        missing.join(", ")
    )))
}
