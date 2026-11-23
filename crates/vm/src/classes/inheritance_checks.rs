//! Structural method checks run before publishing inherited tables, avoiding partially linked classes.

use std::{collections::HashMap, rc::Rc};

use phera_bytecode::{ClassKind, Visibility};
use phera_extension_api::NativeError;

use super::{Method, MethodBody};

/// Private methods occupy independent scopes; public/protected overrides must preserve contracts.
pub(crate) fn check_override(
    class: &str,
    inherited: &Method,
    method: &Method,
) -> Result<(), NativeError> {
    if inherited.visibility == Visibility::Private
        && !inherited.name.eq_ignore_ascii_case("__construct")
    {
        return Ok(());
    }
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
    is_final: bool,
    methods: &HashMap<String, Rc<Method>>,
    registry: &phera_extension_api::Registry,
) -> Result<(), NativeError> {
    if matches!(kind, ClassKind::Interface | ClassKind::Trait) {
        return Ok(());
    }
    let mut missing: Vec<_> = methods
        .values()
        .filter(|method| {
            method.is_abstract && (!is_abstract || method.visibility == Visibility::Private)
        })
        .collect();
    missing.sort_by_key(|method| {
        let name = method.declaring.as_str();
        let native_order = registry.get_class(name).and_then(|class| {
            class
                .methods
                .iter()
                .position(|other| other.name == method.name)
        });
        let source_order = match &method.body {
            MethodBody::User(function) => function.index(),
            _ => usize::MAX,
        };
        (
            name,
            native_order.unwrap_or(source_order),
            method.name.as_str(),
        )
    });
    if missing.is_empty() {
        return Ok(());
    }
    let class = super::trait_methods::display(class);
    let count = missing.len();
    let plural = if count == 1 { "" } else { "s" };
    let mut remaining: Vec<_> = missing
        .into_iter()
        .take(3)
        .map(|method| format!("{}::{}", method.declaring, method.name))
        .collect();
    if count > 3 {
        remaining.push("...".into());
    }
    if matches!(kind, ClassKind::Anonymous | ClassKind::Enum) || is_abstract || is_final {
        let subject = kind.description();
        return Err(NativeError::error(format!(
            "{subject} {class} must implement {count} abstract method{plural} ({})",
            remaining.join(", ")
        )));
    }
    Err(NativeError::error(format!(
        "Class {class} contains {count} abstract method{plural} and must therefore be declared abstract or implement the remaining method{plural} ({})",
        remaining.join(", ")
    )))
}
