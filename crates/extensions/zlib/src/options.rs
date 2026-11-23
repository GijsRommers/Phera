//! zlib stream options are validated before any compressor state is allocated.

use phera_extension_api::{
    ArrayKey, Context, NativeError, PhpArray, Value,
    value::{to_int, to_php_string},
};

use crate::oneshot::argument;

pub(crate) fn properties(value: &Value) -> PhpArray {
    match value {
        Value::Array(options) => (**options).clone(),
        Value::Object(object) => object.to_array(),
        _ => PhpArray::new(),
    }
}

pub(crate) fn integer(
    context: &mut dyn Context,
    options: &PhpArray,
    key: &str,
    default: i64,
) -> Result<i64, NativeError> {
    match options.get(&ArrayKey::str(key)) {
        Some(value) => Ok(to_int(value, context.diagnostics())?),
        None => Ok(default),
    }
}

pub(crate) fn dictionary(
    context: &mut dyn Context,
    options: &PhpArray,
    function: &str,
) -> Result<Option<Vec<u8>>, NativeError> {
    let Some(value) = options.get(&ArrayKey::str("dictionary")) else {
        return Ok(None);
    };
    match &*value.deref() {
        Value::InternedStr(bytes) => Ok(Some(bytes.as_bytes().to_vec())),
        Value::String(bytes) => Ok(Some(bytes.as_bytes().to_vec())),
        Value::Array(values) => {
            let mut dictionary = Vec::new();
            for (_, value) in values.iter() {
                let string = to_php_string(value, context.diagnostics())?;
                let bytes = string.as_bytes();
                if bytes.is_empty() {
                    return Err(argument(
                        function,
                        2,
                        "options",
                        "must not contain empty strings",
                    ));
                }
                if bytes.contains(&0) {
                    return Err(argument(
                        function,
                        2,
                        "options",
                        "must not contain strings with null bytes",
                    ));
                }
                dictionary.extend_from_slice(bytes);
                dictionary.push(0);
            }
            Ok((!dictionary.is_empty()).then_some(dictionary))
        }
        given => Err(NativeError::type_error(format!(
            "{}(): Argument #2 ($options) must be of type zero-terminated string or array, {} given",
            function,
            match given {
                Value::Bool(true) => "true".into(),
                Value::Bool(false) => "false".into(),
                value => value.type_name(),
            }
        ))),
    }
}
