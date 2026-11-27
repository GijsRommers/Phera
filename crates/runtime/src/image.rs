//! AOT `.phb` image integration for [`crate::Runtime`]: deterministic ABI
//! fingerprinting, native inventory extraction, and build-time compilation using
//! the runtime's exact extension registry.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, LazyLock};

use phera_aot::{AotImage, ImageError, ImageTarget};
use phera_bytecode::CompiledUnit;
use phera_extension_api::{ConstantValue, NativeFn, Registry};

use crate::cache::{FileMode, compile_unit_with_registry};
use crate::native_registry;

static COMBINED_ABI_HASH: LazyLock<[u8; 16]> = LazyLock::new(|| {
    let mut state = blake2b_simd::Params::new().hash_length(16).to_state();
    state.update(b"phera-runtime-registries-v1\0");
    state.update(&registry_fingerprint(&native_registry::web()));
    state.update(&registry_fingerprint(&native_registry::cli()));
    let mut combined = [0u8; 16];
    combined.copy_from_slice(state.finalize().as_bytes());
    phera_aot::runtime_abi_hash(combined)
});

/// Computes a deterministic 16-byte fingerprint of one [`Registry`]'s exact
/// extension modules, function signatures/handler kinds, classes, and constants.
pub fn registry_fingerprint(registry: &Registry) -> [u8; 16] {
    let mut state = blake2b_simd::Params::new().hash_length(16).to_state();
    state.update(b"phera-registry-v1\0");
    state.update(phera_extension_api::PHP_VERSION.as_bytes());
    state.update(b"\0");

    let mut extensions: Vec<&str> = registry.extensions().collect();
    extensions.sort_unstable();
    state.update(&(extensions.len() as u32).to_le_bytes());
    for ext in extensions {
        state.update(ext.as_bytes());
        state.update(b"\0");
    }

    let mut functions: Vec<_> = registry.functions().collect();
    functions.sort_by(|a, b| a.name.cmp(b.name));
    state.update(&(functions.len() as u32).to_le_bytes());
    for func in functions {
        state.update(func.name.as_bytes());
        state.update(b"\0");
        let handler_tag: u8 = match func.handler {
            NativeFn::Sync(_) => 0,
            NativeFn::Async(_) => 1,
            NativeFn::DeprecatedSync(_) => 2,
            NativeFn::DeprecatedAsync(_) => 3,
        };
        state.update(&[
            handler_tag,
            u8::from(registry.sync_selector(func.name).is_some()),
        ]);
        state.update(&(func.params.len() as u32).to_le_bytes());
        for param in func.params {
            state.update(param.name.as_bytes());
            state.update(b"\0");
            state.update(param.ty.name().as_bytes());
            state.update(b"\0");
            let flags = u8::from(param.by_ref)
                | (u8::from(param.variadic) << 1)
                | (u8::from(param.optional) << 2)
                | (u8::from(param.nullable) << 3);
            state.update(&[flags]);
        }
    }

    let mut classes: Vec<_> = registry.classes().collect();
    classes.sort_by(|a, b| a.name.cmp(b.name));
    state.update(&(classes.len() as u32).to_le_bytes());
    for class in classes {
        state.update(class.name.as_bytes());
        state.update(b"\0");
        state.update(class.parent.unwrap_or("").as_bytes());
        state.update(b"\0");
        state.update(&(class.interfaces.len() as u32).to_le_bytes());
        for iface in class.interfaces {
            state.update(iface.as_bytes());
            state.update(b"\0");
        }
    }

    let mut constants: Vec<_> = registry.constant_entries().collect();
    constants.sort_by(|a, b| a.0.cmp(b.0));
    state.update(&(constants.len() as u32).to_le_bytes());
    for (name, value, deprecated) in constants {
        state.update(name.as_bytes());
        state.update(b"\0");
        state.update(&[u8::from(deprecated.is_some())]);
        match value {
            ConstantValue::Null => {
                state.update(&[0]);
            }
            ConstantValue::Bool(v) => {
                state.update(&[1, u8::from(*v)]);
            }
            ConstantValue::Int(v) => {
                state.update(&[2]);
                state.update(&v.to_le_bytes());
            }
            ConstantValue::Float(v) => {
                state.update(&[3]);
                state.update(&v.to_bits().to_le_bytes());
            }
            ConstantValue::String(s) => {
                state.update(&[4]);
                state.update(s.as_bytes());
                state.update(b"\0");
            }
            ConstantValue::EmptyArray => {
                state.update(&[5]);
            }
        }
    }

    let mut out = [0u8; 16];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// The runtime ABI hash expected by this Phera binary for `.phb` images.
pub fn runtime_abi_hash() -> [u8; 16] {
    *COMBINED_ABI_HASH
}

/// Opens and ABI-verifies a `.phb` image from disk against this binary's runtime.
pub fn open_image(path: &Path) -> Result<Arc<AotImage>, ImageError> {
    AotImage::open(path, Some(runtime_abi_hash()))
}

/// Native and runtime-shipped inventory for `phera build` closed-world analysis.
#[derive(Debug, Clone)]
pub struct NativeInventory {
    pub functions: BTreeSet<String>,
    pub classes: BTreeSet<String>,
    /// Lowercased parent/interface edges for native classes and bundled PHP
    /// adapter/runtime classes (e.g. `Phera\Symfony\Runtime`).
    pub supertypes: BTreeMap<String, Vec<String>>,
}

/// Returns the lowercased native functions, classes, and hierarchy edges
/// (including runtime-shipped bundled PHP classes) registered for `target`.
pub fn native_inventory(target: ImageTarget) -> NativeInventory {
    let (registry, mode) = match target {
        ImageTarget::Web => (native_registry::web(), FileMode::Web),
        ImageTarget::Cli => (native_registry::cli(), FileMode::Cli),
    };
    let mut functions: BTreeSet<String> = registry
        .functions()
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect();
    // Engine intrinsics handled directly by the VM without extension registration.
    for intrinsic in [
        "array_map",
        "is_callable",
        "call_user_func",
        "call_user_func_array",
        "get_defined_vars",
        "compact",
        "extract",
        "define",
        "defined",
        "constant",
        "function_exists",
        "func_num_args",
        "func_get_args",
        "func_get_arg",
        "error_reporting",
        "set_error_handler",
        "restore_error_handler",
        "set_exception_handler",
        "restore_exception_handler",
        "get_error_handler",
        "get_exception_handler",
        "trigger_error",
        "user_error",
        "clone",
        "phera\\snapshot",
        "get_class",
        "get_called_class",
        "get_parent_class",
        "class_exists",
        "interface_exists",
        "trait_exists",
        "enum_exists",
        "class_alias",
        "is_a",
        "is_subclass_of",
        "method_exists",
        "property_exists",
        "get_class_methods",
        "get_class_vars",
        "get_object_vars",
        "get_declared_classes",
        "get_declared_interfaces",
        "get_declared_traits",
        "spl_autoload_register",
        "spl_autoload_unregister",
        "spl_autoload_functions",
        "spl_autoload_call",
        "ob_start",
        "ob_clean",
        "ob_end_clean",
        "ob_end_flush",
        "ob_flush",
        "ob_get_clean",
        "ob_get_contents",
        "ob_get_flush",
        "ob_get_length",
        "ob_get_level",
        "ob_get_status",
        "ob_list_handlers",
        "gc_enable",
        "gc_disable",
        "gc_enabled",
        "gc_collect_cycles",
        "gc_status",
        "extension_loaded",
        "get_loaded_extensions",
        "get_extension_funcs",
        "get_defined_functions",
        "get_defined_constants",
        "phpversion",
        "assert",
    ] {
        if phera_vm::is_intrinsic_function(intrinsic) {
            functions.insert(intrinsic.to_owned());
        }
    }
    let mut classes: BTreeSet<String> = BTreeSet::new();
    let mut supertypes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in registry.classes() {
        let lower = entry.name.to_ascii_lowercase();
        classes.insert(lower.clone());
        let mut parents = Vec::new();
        if let Some(parent) = entry.parent {
            parents.push(parent.to_ascii_lowercase());
        }
        for iface in entry.interfaces {
            parents.push(iface.to_ascii_lowercase());
        }
        if !parents.is_empty() {
            parents.sort();
            parents.dedup();
            supertypes.insert(lower, parents);
        }
    }
    // Include bundled PHP classes shipped in the Phera binary (such as
    // `Phera\Symfony\Runtime extends Symfony\Component\Runtime\SymfonyRuntime`)
    // so closed-world hierarchy analysis never marks runtime-subclassed types
    // as EFFECTIVELY_FINAL.
    for bundled in registry.bundled_sources() {
        let unit =
            compile_unit_with_registry(&registry, mode, bundled.source.as_bytes(), bundled.file)
                .unwrap_or_else(|error| {
                    panic!(
                        "shipped bundled PHP source `{}` failed to compile: {}",
                        bundled.file, error.message
                    )
                });
        for class in &unit.classes {
            if class.kind == phera_bytecode::ClassKind::Anonymous {
                continue;
            }
            let lower = class.name.to_ascii_lowercase();
            classes.insert(lower.clone());
            let mut parents = Vec::new();
            if let Some(parent) = class.parent
                && let Some(sym) = unit.symbols.get(parent.index())
            {
                parents.push(sym.name.to_ascii_lowercase());
            }
            for iface in &class.interfaces {
                if let Some(sym) = unit.symbols.get(iface.index()) {
                    parents.push(sym.name.to_ascii_lowercase());
                }
            }
            if !parents.is_empty() {
                let slot = supertypes.entry(lower).or_default();
                slot.extend(parents);
                slot.sort();
                slot.dedup();
            }
        }
    }
    NativeInventory {
        functions,
        classes,
        supertypes,
    }
}

/// Compiles PHP source bytes using the exact native function inventory and
/// constant-folding rules for `target` (`Web` or `Cli`).
pub fn compile_bytes_for_target(
    source: &[u8],
    file: &str,
    target: ImageTarget,
) -> Result<CompiledUnit, phera_compiler::CompileError> {
    let (registry, mode) = match target {
        ImageTarget::Web => (native_registry::web(), FileMode::Web),
        ImageTarget::Cli => (native_registry::cli(), FileMode::Cli),
    };
    compile_unit_with_registry(&registry, mode, source, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_inventory_includes_shipped_symfony_runtime_subclass() {
        let inv = native_inventory(ImageTarget::Web);
        assert!(inv.classes.contains("phera\\symfony\\runtime"));
        let parents = inv
            .supertypes
            .get("phera\\symfony\\runtime")
            .expect("Phera\\Symfony\\Runtime must declare its SymfonyRuntime parent");
        assert!(
            parents
                .iter()
                .any(|p| p == "symfony\\component\\runtime\\symfonyruntime"),
            "unexpected parents: {parents:?}"
        );
    }
}
