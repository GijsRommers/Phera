//! `str_getcsv('"a,b",c', escape: '')` parses the same record as `fgetcsv()`.
//!
//! String input is one record, including any literal newlines in the middle.
//! A stream supplies another line only while a quoted field remains open.

use std::io;

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

mod controls;
mod encoding;

pub use controls::Controls;

use encoding::Encoding;

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

#[derive(Clone, Copy)]
pub struct Dialect {
    separator: u8,
    enclosure: u8,
    escape: Option<u8>,
    encoding: Encoding,
}

impl Dialect {
    /// Validate the same dialect parameters for both string and stream functions.
    pub fn from_arguments(
        context: &mut dyn Context,
        function: &str,
        arguments: &[Value],
        start: usize,
    ) -> Result<Self, NativeError> {
        let controls = Controls::default().with_arguments(
            context,
            function,
            arguments,
            start,
            Some(" as its default value will change"),
        )?;
        Ok(controls.dialect(context))
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
        let mut end = self.encoding.line_end(&line);
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
                && self.encoding.character_width(&line[leading..end]) == 1;
            if quoted {
                offset = leading + 1;
                loop {
                    if offset == end {
                        field.extend_from_slice(&line[end..]);
                        line = next_line()?;
                        offset = 0;
                        end = self.encoding.line_end(&line);
                        if line.is_empty() {
                            break;
                        }
                        continue;
                    }
                    let width = self.encoding.character_width(&line[offset..end]);
                    let byte = line[offset];
                    offset += width;
                    if width == 1 && byte == self.enclosure {
                        if offset < end
                            && line[offset] == self.enclosure
                            && self.encoding.character_width(&line[offset..end]) == 1
                        {
                            offset += 1;
                        } else {
                            break;
                        }
                    } else if width == 1 && Some(byte) == self.escape && offset < end {
                        field.push(byte);
                        let after = offset + self.encoding.character_width(&line[offset..end]);
                        field.extend_from_slice(&line[offset..after]);
                        offset = after;
                        continue;
                    }
                    field.extend_from_slice(&line[offset - width..offset]);
                }
            }
            let start = offset;
            while offset < end {
                let width = self.encoding.character_width(&line[offset..end]);
                if width == 1 && line[offset] == self.separator {
                    break;
                }
                offset += width;
            }
            let rest = &line[start..offset];
            field.extend_from_slice(if quoted {
                rest
            } else {
                &rest[..self.encoding.line_end(rest)]
            });
            fields.push(Some(field));
            if offset == end {
                return Ok(fields);
            }
            offset += 1;
        }
    }
}

pub fn values(fields: Vec<Option<Vec<u8>>>) -> NativeResult {
    fields_array(fields).map(Value::from)
}

pub fn fields_array(fields: Vec<Option<Vec<u8>>>) -> Result<PhpArray, NativeError> {
    let mut values = PhpArray::new();
    for field in fields {
        values.push(field.map_or(Value::Null, Value::from))?;
    }
    Ok(values)
}

fn str_getcsv(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let dialect = Dialect::from_arguments(context, "str_getcsv", arguments, 1)?;
    let input = args::bytes(arguments, 0);
    let fields = dialect
        .decode(input.to_vec(), || Ok(Vec::new()))
        .map_err(|error| NativeError::error(error.to_string()))?;
    values(fields)
}

fn space(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte == 11
}
