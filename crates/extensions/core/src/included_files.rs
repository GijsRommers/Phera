//! require 'bootstrap.php'; get_included_files() lists the entry script, then bootstrap.php.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Registry, Value};

pub(super) fn register(registry: &mut Registry) {
    for name in ["get_included_files", "get_required_files"] {
        registry.function(FunctionEntry::sync(name, &[], included_files));
        registry.describe_function(name, "array", Some("Core"));
    }
}

fn included_files(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(context.included_files().into())
}
