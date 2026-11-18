//! `putenv("MODE=test"); echo getenv("MODE");` changes only the current request.
//! As in PHP-FPM, the next request starts with the original environment. The process
//! itself is shared by concurrent requests and its environment is never mutated.
//! Child processes must receive the request's `effective_environment` when spawning
//! becomes available. The startup snapshots in $_ENV and $_SERVER stay independent.

use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use phera_dotenv::Environment;
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

#[derive(Default)]
pub struct RequestEnvironment {
    base: Arc<Environment>,
    // A tombstone also hides the original position after an unset and re-add.
    replacements: HashMap<Vec<u8>, Option<Vec<u8>>>,
    appended: IndexMap<Vec<u8>, Vec<u8>>,
}

impl RequestEnvironment {
    pub fn new(base: Arc<Environment>) -> Self {
        Self {
            base,
            ..Self::default()
        }
    }

    fn lookup(&self, name: &[u8]) -> Option<&[u8]> {
        if let Some(value) = self.appended.get(name) {
            return Some(value);
        }
        match self.replacements.get(name) {
            Some(value) => value.as_deref(),
            None => self.base.get(name),
        }
    }

    fn assign(&mut self, name: &[u8], value: Option<&[u8]>) {
        let inherited = self.base.get(name).is_some();
        if inherited && (value.is_none() || self.replacements.get(name) != Some(&None)) {
            self.replacements
                .insert(name.to_vec(), value.map(<[u8]>::to_vec));
            if value.is_none() {
                self.appended.shift_remove(name);
            }
            return;
        }
        // PHP first releases an earlier putenv entry. A request-created name
        // therefore moves to the end on reassignment.
        if !inherited || value.is_none() {
            self.appended.shift_remove(name);
        }
        if let Some(value) = value {
            self.appended.insert(name.to_vec(), value.to_vec());
        }
    }

    /// The exact ordered name/value pairs to pass to a future child process.
    pub fn effective_environment(&self) -> Vec<(&[u8], &[u8])> {
        self.base
            .iter()
            .filter_map(|(name, original)| match self.replacements.get(name) {
                Some(value) => value.as_deref().map(|value| (name, value)),
                None => Some((name, original)),
            })
            .chain(
                self.appended
                    .iter()
                    .map(|(n, v)| (n.as_slice(), v.as_slice())),
            )
            .collect()
    }
}

/// PHP's environment importer skips names containing spaces, dots or opening brackets.
pub fn php_array<'a>(entries: impl IntoIterator<Item = (&'a [u8], &'a [u8])>) -> PhpArray {
    let mut array = PhpArray::new();
    for (name, value) in entries {
        if !name.is_empty() && !name.iter().any(|b| matches!(b, b' ' | b'.' | b'[')) {
            let key = ArrayKey::from_value(&Value::from(name)).expect("string array key");
            array.insert(key, Value::from(value));
        }
    }
    array
}

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "getenv",
        const {
            &[
                Param::new("name", ParamType::String).nullable().optional(),
                Param::new("local_only", ParamType::Bool).optional(),
            ]
        },
        getenv,
    ));
    registry.function(FunctionEntry::sync(
        "putenv",
        const { &[Param::new("assignment", ParamType::String)] },
        putenv,
    ));
}

fn getenv(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let env = context.state().get_or_default::<RequestEnvironment>();
    let Some(name) = args::string(arguments, 0) else {
        return Ok(Value::from(php_array(env.effective_environment())));
    };
    let name = c_string(name.as_bytes());
    // Darwin libc stops reading a lookup name at '='. glibc compares the
    // requested prefix against the full NAME=value string, including any '='.
    let value = if let Some(equal) = name.iter().position(|b| *b == b'=') {
        let value = env.lookup(&name[..equal]);
        if cfg!(target_vendor = "apple") {
            value
        } else {
            value.and_then(|v| v.strip_prefix(&name[equal + 1..])?.strip_prefix(b"="))
        }
    } else {
        env.lookup(name)
    };
    Ok(value.map_or(Value::Bool(false), Value::from))
}

fn putenv(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let assignment = args::bytes(arguments, 0);
    if assignment.is_empty() || assignment.starts_with(b"=") {
        return Err(NativeError::value_error(
            "putenv(): Argument #1 ($assignment) must have a valid syntax",
        ));
    }
    let setting = c_string(assignment);
    let (name, value) = match setting.iter().position(|b| *b == b'=') {
        Some(index) => (&setting[..index], Some(&setting[index + 1..])),
        None => (setting, None),
    };
    context
        .state()
        .get_or_default::<RequestEnvironment>()
        .assign(name, value);
    Ok(Value::Bool(true))
}

fn c_string(bytes: &[u8]) -> &[u8] {
    bytes.split(|b| *b == 0).next().unwrap_or_default()
}
