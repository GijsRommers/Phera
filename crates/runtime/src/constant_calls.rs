//! Calls such as `constant('PHP_INT_MAX')` can use the immutable registry.
//! Request-defined constants and warnings still need a runtime call.

use phera_bytecode::Constant;
use phera_extension_api::{ConstantValue, Registry};

pub(super) fn fold(registry: &Registry, name: &str, argument: &[u8]) -> Option<Constant> {
    match name {
        "function_exists" | "is_callable" => {
            let candidate = std::str::from_utf8(argument).ok()?;
            (registry.get_function(candidate).is_some()
                || phera_vm::is_intrinsic_function(candidate))
            .then_some(Constant::Bool(true))
        }
        "extension_loaded" => {
            let candidate = std::str::from_utf8(argument).ok()?;
            // Phera fixes its extension inventory when the runtime starts.
            let loaded = registry
                .extensions()
                .any(|module| module.eq_ignore_ascii_case(candidate));
            Some(Constant::Bool(loaded))
        }
        "constant" => {
            let candidate = std::str::from_utf8(argument).ok()?;
            persistent_scalar(registry.persistent_constant(candidate)?)
        }
        "dirname" if cfg!(unix) && argument.starts_with(b"/") && !argument.contains(&0) => {
            let parent = phera_ext_streams::dirname_parent(argument);
            parent
                .starts_with(b"/")
                .then(|| Constant::String(parent.to_vec()))
        }
        _ => None,
    }
}

fn persistent_scalar(value: ConstantValue) -> Option<Constant> {
    Some(match value {
        ConstantValue::Null => Constant::Null,
        ConstantValue::Bool(value) => Constant::Bool(value),
        ConstantValue::Int(value) => Constant::Int(value),
        ConstantValue::Float(value) => Constant::Float(value),
        ConstantValue::String(value) => Constant::String(value.as_bytes().to_vec()),
        ConstantValue::EmptyArray => return None,
    })
}
