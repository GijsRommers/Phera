//! `$_SERVER['SCRIPT_NAME']` and the other CLI entries override inherited names.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use phera_value::{ArrayKey, PhpArray, Value};

pub(crate) fn cli_server(mut server: PhpArray, script: &Path, arguments: &[String]) -> PhpArray {
    let filename = script.canonicalize().unwrap_or_else(|_| script.to_owned());
    let script = arguments
        .first()
        .map_or(script.as_os_str().as_encoded_bytes(), |s| s.as_bytes());
    let mut set = |name: &str, value: Value| {
        server.insert(ArrayKey::str(name), value);
    };
    for key in ["PHP_SELF", "SCRIPT_NAME"] {
        set(key, Value::from(script));
    }
    for key in ["SCRIPT_FILENAME", "PATH_TRANSLATED"] {
        set(key, Value::from(filename.as_os_str().as_encoded_bytes()));
    }
    set("DOCUMENT_ROOT", Value::from(""));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    set("REQUEST_TIME_FLOAT", Value::Float(now.as_secs_f64()));
    set("REQUEST_TIME", Value::Int(now.as_secs() as i64));
    let mut argv = PhpArray::new();
    for argument in arguments {
        let _ = argv.push(Value::from(argument.as_str()));
    }
    set("argv", Value::from(argv));
    set("argc", Value::Int(arguments.len() as i64));
    server
}
