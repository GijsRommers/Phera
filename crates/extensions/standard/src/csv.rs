//! `str_getcsv('"a,b",c', escape: '')` parses the same record as `fgetcsv()`.
//!
//! String input is one record, including any literal newlines in the middle.
//! A stream supplies another line only while a quoted field remains open.

use std::io;

use phera_extension_api::value::PhpString;
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

pub const SEPARATOR: Param =
    Param::new("separator", ParamType::String).with_default(ConstantValue::String(","));
pub const ENCLOSURE: Param =
    Param::new("enclosure", ParamType::String).with_default(ConstantValue::String("\""));
pub const ESCAPE: Param =
    Param::new("escape", ParamType::String).with_default(ConstantValue::String("\\"));

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "str_getcsv",
        const {
            &[
                Param::new("string", ParamType::String),
                SEPARATOR,
                ENCLOSURE,
                ESCAPE,
            ]
        },
        str_getcsv,
    ));
    registry.describe_function("str_getcsv", "array", None);
}

pub struct Dialect {
    separator: u8,
    enclosure: u8,
    escape: Option<u8>,
    utf8: bool,
}

impl Dialect {
    /// Validate the same dialect parameters for both string and stream functions.
    pub fn from_arguments(
        context: &mut dyn Context,
        function: &str,
        arguments: &[Value],
        start: usize,
    ) -> Result<Self, NativeError> {
        let separator = byte(function, arguments, start, "separator", b',')?;
        let enclosure = byte(function, arguments, start + 1, "enclosure", b'"')?;
        let escape = match args::string(arguments, start + 2).map(PhpString::as_bytes) {
            None => {
                context.deprecated(format!(
                    "{function}(): the $escape parameter must be provided as its default value will change"
                ));
                Some(b'\\')
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
            utf8: crate::locale::ctype_is_utf8(context),
        })
    }

    pub fn separator(&self) -> u8 {
        self.separator
    }

    /// Encode a field and double its unescaped enclosures.
    pub fn encode(&self, field: &[u8], output: &mut Vec<u8>) {
        if !field.iter().any(|&byte| {
            byte == self.separator
                || byte == self.enclosure
                || Some(byte) == self.escape
                || matches!(byte, b'\n' | b'\r' | b'\t' | b' ')
        }) {
            output.extend_from_slice(field);
            return;
        }
        output.push(self.enclosure);
        let mut escaped = false;
        for &byte in field {
            if Some(byte) == self.escape {
                escaped = true;
            } else if byte == self.enclosure && !escaped {
                output.push(byte);
            } else {
                escaped = false;
            }
            output.push(byte);
        }
        output.push(self.enclosure);
    }

    /// Decode the starting bytes and fetch continuation lines for open enclosures.
    pub fn decode(
        &self,
        mut line: Vec<u8>,
        mut next_line: impl FnMut() -> io::Result<Vec<u8>>,
    ) -> io::Result<Vec<Option<Vec<u8>>>> {
        let mut end = line_end(&line);
        if end == 0 {
            return Ok(vec![None]);
        }
        let mut fields = Vec::new();
        let mut offset = 0;
        loop {
            let mut field = Vec::new();
            let mut leading = offset;
            while leading < end && line[leading] != self.separator && space(line[leading]) {
                leading += 1;
            }
            let quoted = leading < end
                && line[leading] == self.enclosure
                && self.character_width(&line[leading..end]) == 1;
            if quoted {
                offset = leading + 1;
                loop {
                    if offset == end {
                        field.extend_from_slice(&line[end..]);
                        line = next_line()?;
                        offset = 0;
                        end = line_end(&line);
                        if line.is_empty() {
                            break;
                        }
                        continue;
                    }
                    let width = self.character_width(&line[offset..end]);
                    let byte = line[offset];
                    offset += width;
                    if width == 1 && byte == self.enclosure {
                        if offset < end
                            && line[offset] == self.enclosure
                            && self.character_width(&line[offset..end]) == 1
                        {
                            offset += 1;
                        } else {
                            break;
                        }
                    } else if width == 1 && Some(byte) == self.escape && offset < end {
                        field.push(byte);
                        let after = offset + self.character_width(&line[offset..end]);
                        field.extend_from_slice(&line[offset..after]);
                        offset = after;
                        continue;
                    }
                    field.extend_from_slice(&line[offset - width..offset]);
                }
            }
            let start = offset;
            while offset < end {
                let width = self.character_width(&line[offset..end]);
                if width == 1 && line[offset] == self.separator {
                    break;
                }
                offset += width;
            }
            let rest = &line[start..offset];
            field.extend_from_slice(if quoted {
                rest
            } else {
                &rest[..line_end(rest)]
            });
            fields.push(Some(field));
            if offset == end {
                return Ok(fields);
            }
            offset += 1;
        }
    }

    /// PHP tests delimiter bytes only at character boundaries in a UTF-8 locale.
    fn character_width(&self, input: &[u8]) -> usize {
        if !self.utf8 {
            return 1;
        }
        let width = match input.first() {
            Some(0xC2..=0xDF) => 2,
            Some(0xE0..=0xEF) => 3,
            Some(0xF0..=0xF4) => 4,
            _ => return 1,
        };
        if input
            .get(..width)
            .is_some_and(|bytes| std::str::from_utf8(bytes).is_ok())
        {
            width
        } else {
            1
        }
    }
}

pub fn values(fields: Vec<Option<Vec<u8>>>) -> NativeResult {
    let mut values = PhpArray::new();
    for field in fields {
        values.push(field.map_or(Value::Null, Value::from))?;
    }
    Ok(Value::from(values))
}

fn str_getcsv(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let dialect = Dialect::from_arguments(context, "str_getcsv", arguments, 1)?;
    let input = args::string(arguments, 0).map_or(&[][..], PhpString::as_bytes);
    let fields = dialect
        .decode(input.to_vec(), || Ok(Vec::new()))
        .map_err(|error| NativeError::error(error.to_string()))?;
    values(fields)
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

fn line_end(line: &[u8]) -> usize {
    let trimmed = line.strip_suffix(b"\n").unwrap_or(line);
    trimmed.strip_suffix(b"\r").unwrap_or(trimmed).len()
}

fn space(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte == 11
}
