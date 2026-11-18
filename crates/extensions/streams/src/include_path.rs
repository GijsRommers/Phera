//! `stream_resolve_include_path('config.php')` searches the request's include path.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Registry, Value, args};

use crate::errors::check_path;
use crate::handles::FILENAME;
use crate::request::{bytes, files, path};
use crate::wrapper::Target;

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "stream_resolve_include_path",
        &[FILENAME],
        resolve,
    ));
}

fn resolve(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let name = args::bytes(arguments, 0);
    check_path("stream_resolve_include_path", "#1 ($filename)", name)?;
    let include_path = context.include_path();
    let script = context.script_path();
    let found = match Target::parse(name) {
        Target::File(file) if file != name || name.is_empty() => files(context)
            .resolve(path(file))
            .ok()
            .and_then(|p| p.canonicalize().ok()),
        Target::File(file) => files(context).search(file, &include_path, script.as_deref()),
        _ => None,
    };
    Ok(found.map_or(Value::Bool(false), |path| {
        Value::from(bytes(&path).to_vec())
    }))
}
