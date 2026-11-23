//! `request_parse_body(['upload_max_filesize' => '1M'])` overrides the
//! corresponding INI setting for this call only.

use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value, ini};

#[derive(Clone, Copy)]
pub(super) struct FormLimits {
    pub(super) post_max_size: i64,
    pub(super) max_input_vars: i64,
    pub(super) max_multipart_body_parts: i64,
    pub(super) max_file_uploads: i64,
    pub(super) upload_max_filesize: i64,
    pub(super) file_uploads: bool,
}

impl Default for FormLimits {
    fn default() -> Self {
        Self {
            post_max_size: 8 * 1024 * 1024,
            max_input_vars: 1000,
            max_multipart_body_parts: -1,
            max_file_uploads: 20,
            upload_max_filesize: 2 * 1024 * 1024,
            file_uploads: true,
        }
    }
}

impl FormLimits {
    pub(super) fn from_options(
        context: &mut dyn Context,
        options: Option<&PhpArray>,
    ) -> Result<Self, NativeError> {
        let mut limits = Self::from_ini(context);
        if let Some(options) = options {
            for (key, value) in options.iter() {
                let option = limits.option(&key)?;
                *option = match &*value.deref() {
                    Value::Int(value) => *value,
                    Value::InternedStr(value) => {
                        let (number, warning) = ini::parse_quantity(value.as_bytes());
                        if let Some(warning) = warning {
                            context.warning(warning);
                        }
                        number
                    }
                    Value::String(value) => {
                        let (number, warning) = ini::parse_quantity(value.as_bytes());
                        if let Some(warning) = warning {
                            context.warning(warning);
                        }
                        number
                    }
                    Value::Bool(value) => return Err(invalid_value(&value.to_string())),
                    value => return Err(invalid_value(&value.type_name())),
                };
            }
        }
        Ok(limits)
    }

    fn from_ini(context: &dyn Context) -> Self {
        let defaults = Self::default();
        let setting = |key: &str, default, parse: fn(&[u8]) -> i64| {
            context
                .ini_get(key.as_bytes())
                .map_or(default, |value| parse(value.as_bytes()))
        };
        let quantity = |value: &[u8]| ini::parse_quantity(value).0;
        Self {
            post_max_size: setting("post_max_size", defaults.post_max_size, quantity),
            max_input_vars: setting("max_input_vars", defaults.max_input_vars, ini::integer),
            max_multipart_body_parts: setting(
                "max_multipart_body_parts",
                defaults.max_multipart_body_parts,
                ini::integer,
            ),
            max_file_uploads: setting("max_file_uploads", defaults.max_file_uploads, ini::integer),
            upload_max_filesize: setting(
                "upload_max_filesize",
                defaults.upload_max_filesize,
                quantity,
            ),
            file_uploads: context
                .ini_get(b"file_uploads")
                .is_none_or(|value| ini::boolean(value.as_bytes())),
        }
    }

    fn option(&mut self, key: &ArrayKey) -> Result<&mut i64, NativeError> {
        let Some(key) = key.as_str_bytes() else {
            return Err(NativeError::value_error(
                "Invalid integer key in $options argument",
            ));
        };
        if key.is_empty() {
            return Err(NativeError::value_error(
                "Invalid empty string key in $options argument",
            ));
        }
        match key.to_ascii_lowercase().as_slice() {
            b"post_max_size" => Ok(&mut self.post_max_size),
            b"max_input_vars" => Ok(&mut self.max_input_vars),
            b"max_multipart_body_parts" => Ok(&mut self.max_multipart_body_parts),
            b"max_file_uploads" => Ok(&mut self.max_file_uploads),
            b"upload_max_filesize" => Ok(&mut self.upload_max_filesize),
            _ => {
                let shown = key.split(|&byte| byte == 0).next().unwrap_or_default();
                Err(NativeError::throw_bytes(
                    "ValueError",
                    [b"Invalid key \"", shown, b"\" in $options argument"].concat(),
                ))
            }
        }
    }

    pub(super) fn part_limit(self) -> i64 {
        if self.max_multipart_body_parts < 0 {
            self.max_input_vars.wrapping_add(self.max_file_uploads)
        } else {
            self.max_multipart_body_parts
        }
    }
}

fn invalid_value(name: &str) -> NativeError {
    NativeError::value_error(format!("Invalid {name} value in $options argument"))
}
