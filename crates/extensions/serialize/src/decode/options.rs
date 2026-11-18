//! `unserialize($data, ['allowed_classes' => ['Example'], 'max_depth' => 10])`.

use std::collections::HashSet;

use phera_extension_api::serialization::SerializationSettings;
use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value};

#[derive(Default)]
enum AllowedClasses {
    #[default]
    All,
    List(HashSet<Vec<u8>>),
}

pub(super) struct Options {
    allowed: AllowedClasses,
    pub(super) max_depth: usize,
}

impl Options {
    pub(super) async fn read(
        context: &mut dyn Context,
        options: Option<&PhpArray>,
    ) -> Result<Self, NativeError> {
        let mut result = Self {
            allowed: AllowedClasses::All,
            max_depth: context
                .state()
                .get::<SerializationSettings>()
                .map_or(4096, |settings| settings.max_depth),
        };
        let Some(options) = options else {
            return Ok(result);
        };
        if let Some(option) = options.get(&ArrayKey::str("allowed_classes")) {
            let option = option.try_deref()?.clone();
            result.allowed = match option {
                Value::Bool(true) => AllowedClasses::All,
                Value::Bool(false) => AllowedClasses::List(HashSet::new()),
                Value::Array(names) => {
                    AllowedClasses::List(read_class_list(context, &names).await?)
                }
                other => {
                    return Err(NativeError::type_error(format!(
                        "unserialize(): Option \"allowed_classes\" must be of type array|bool, {} given",
                        other.type_name()
                    )));
                }
            };
        }
        if let Some(depth) = options.get(&ArrayKey::str("max_depth")) {
            let depth = depth.try_deref()?;
            let Value::Int(depth) = &*depth else {
                return Err(NativeError::type_error(format!(
                    "unserialize(): Option \"max_depth\" must be of type int, {} given",
                    depth.type_name()
                )));
            };
            result.max_depth = usize::try_from(*depth).map_err(|_| {
                NativeError::value_error(
                    "unserialize(): Option \"max_depth\" must be greater than or equal to 0",
                )
            })?;
        }
        Ok(result)
    }

    pub(super) fn allows(&self, name: &[u8]) -> bool {
        match &self.allowed {
            AllowedClasses::All => true,
            AllowedClasses::List(list) => list.contains(&name.to_ascii_lowercase()),
        }
    }
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
                    "unserialize(): Option \"allowed_classes\" must be an array of class names, {} given",
                    value.type_name()
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
                "unserialize(): Option \"allowed_classes\" must be an array of class names, \"{}\" given",
                String::from_utf8_lossy(name)
            )));
        }
        names.insert(name.as_bytes().to_ascii_lowercase());
    }
    Ok(names)
}
