//! `$file->setCsvControl(';', '"', '')` uses the same byte controls as `str_getcsv()`.

use phera_extension_api::{Context, NativeError, PhpString, Value, args};

use super::{Dialect, Encoding};

/// Validated CSV bytes can be stored independently of the current request locale.
#[derive(Clone, Copy)]
pub struct Controls {
    separator: u8,
    enclosure: u8,
    escape: Option<u8>,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            separator: b',',
            enclosure: b'"',
            escape: Some(b'\\'),
        }
    }
}

impl Controls {
    /// Apply call arguments and the caller's rule for deprecating an omitted escape.
    pub fn with_arguments(
        self,
        context: &mut dyn Context,
        function: &str,
        arguments: &[Value],
        start: usize,
        missing_escape: Option<&str>,
    ) -> Result<Self, NativeError> {
        let separator = byte(function, arguments, start, "separator", self.separator)?;
        let enclosure = byte(function, arguments, start + 1, "enclosure", self.enclosure)?;
        let escape = match args::string(arguments, start + 2).map(PhpString::as_bytes) {
            None => {
                if let Some(reason) = missing_escape {
                    context.deprecated(format!(
                        "{function}(): the $escape parameter must be provided{reason}"
                    ));
                }
                self.escape
            }
            Some([]) => None,
            Some([byte]) => Some(*byte),
            _ => {
                return Err(NativeError::value_error(format!(
                    "{function}(): Argument #{} ($escape) must be empty or a single character",
                    start + 3
                )));
            }
        };
        Ok(Self {
            separator,
            enclosure,
            escape,
        })
    }

    pub fn dialect(self, context: &mut dyn Context) -> Dialect {
        Dialect {
            separator: self.separator,
            enclosure: self.enclosure,
            escape: self.escape,
            encoding: Encoding::for_locale(crate::locale::ctype_name(context).as_bytes()),
        }
    }

    /// Arguments keep NUL bytes, while getCsvControl() exposes C strings.
    pub fn arguments(self) -> [Value; 3] {
        self.bytes()
            .map(|value| Value::from(value.into_iter().collect::<Vec<_>>()))
    }

    pub fn strings(self) -> [PhpString; 3] {
        self.bytes().map(|value| match value {
            Some(byte) if byte != 0 => PhpString::from(&[byte][..]),
            _ => PhpString::new(),
        })
    }

    fn bytes(self) -> [Option<u8>; 3] {
        [Some(self.separator), Some(self.enclosure), self.escape]
    }
}

fn byte(
    function: &str,
    arguments: &[Value],
    index: usize,
    name: &str,
    default: u8,
) -> Result<u8, NativeError> {
    match args::string(arguments, index).map(PhpString::as_bytes) {
        None => Ok(default),
        Some([byte]) => Ok(*byte),
        _ => Err(NativeError::value_error(format!(
            "{function}(): Argument #{} (${name}) must be a single character",
            index + 1
        ))),
    }
}
