//! `unserialize($data, ['allowed_classes' => ['Example'], 'max_depth' => 10])`.

use std::collections::HashSet;

use phera_extension_api::serialization::{ActiveUnserializePolicy, UnserializePolicy};
use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value};

pub(super) async fn read(
    context: &mut dyn Context,
    options: Option<&PhpArray>,
) -> Result<UnserializePolicy, NativeError> {
    let mut result = ActiveUnserializePolicy::current(context);
    let Some(options) = options else {
        return Ok(result);
    };
    result.allow_every_class();
    if let Some(option) = options.get(&ArrayKey::str("allowed_classes")) {
        let option = option.try_deref()?.clone();
        match option {
            Value::Bool(true) => {}
            Value::Bool(false) => result.restrict_to(HashSet::new()),
            Value::Array(names) => result.restrict_to(read_class_list(context, &names).await?),
            other => {
                return Err(NativeError::type_error(format!(
                    "unserialize(): Option \"allowed_classes\" must be of type \
                         array|bool, {} given",
                    option_type(&other)
                )));
            }
        }
    }
    if let Some(depth) = options.get(&ArrayKey::str("max_depth")) {
        let depth = depth.try_deref()?;
        let Value::Int(depth) = &*depth else {
            return Err(NativeError::type_error(format!(
                "unserialize(): Option \"max_depth\" must be of type int, {} given",
                option_type(&depth)
            )));
        };
        result.depth = 0;
        result.max_depth = usize::try_from(*depth).map_err(|_| {
            NativeError::value_error(
                "unserialize(): Option \"max_depth\" must be greater than or equal to 0",
            )
        })?;
    }
    Ok(result)
}

async fn read_class_list(
    context: &mut dyn Context,
    values: &PhpArray,
) -> Result<HashSet<Vec<u8>>, NativeError> {
    let mut names = HashSet::with_capacity(values.len());
    for value in values.values() {
        let value = value.try_deref()?.clone();
        let name = match value {
            Value::String(name) => name,
            Value::Object(_) => context.stringify(&value).await?,
            _ => {
                return Err(NativeError::type_error(format!(
                    "unserialize(): Option \"allowed_classes\" must be an array of class \
                     names, {} given",
                    option_type(&value)
                )));
            }
        };
        if !crate::wire::valid_class(name.as_bytes()) {
            let name = name
                .as_bytes()
                .split(|&byte| byte == 0)
                .next()
                .unwrap_or_default();
            return Err(NativeError::value_error(format!(
                "unserialize(): Option \"allowed_classes\" must be an array of class \
                 names, \"{}\" given",
                String::from_utf8_lossy(name)
            )));
        }
        names.insert(name.as_bytes().to_ascii_lowercase());
    }
    Ok(names)
}

fn option_type(value: &Value) -> String {
    match &*value.deref() {
        Value::Bool(value) => value.to_string(),
        _ => value.type_name(),
    }
}
