//! `fputcsv($file, ['a,b', 'c'], escape: '')` writes standard CSV quoting.

use std::io;

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args, value::to_php_string,
};

use crate::handles::{LENGTH, STREAM, read_amount, read_notice};
use crate::resource::{operate, stream_arg, with_stream};
use crate::stream::Stream;
use crate::transfer::write;

const SEPARATOR: Param =
    Param::new("separator", ParamType::String).with_default(ConstantValue::String(","));
const ENCLOSURE: Param =
    Param::new("enclosure", ParamType::String).with_default(ConstantValue::String("\""));
const ESCAPE: Param =
    Param::new("escape", ParamType::String).with_default(ConstantValue::String("\\"));

pub fn register(registry: &mut Registry) {
    const READ: &[Param] = &[STREAM, LENGTH, SEPARATOR, ENCLOSURE, ESCAPE];
    const WRITE: &[Param] = &[
        STREAM,
        Param::new("fields", ParamType::Array),
        SEPARATOR,
        ENCLOSURE,
        ESCAPE,
        Param::new("eol", ParamType::String).with_default(ConstantValue::String("\n")),
    ];
    registry.function(FunctionEntry::asynchronous("fgetcsv", READ, get));
    registry.function(FunctionEntry::asynchronous("fputcsv", WRITE, put));
}

#[derive(Clone, Copy)]
struct Dialect {
    separator: u8,
    enclosure: u8,
    escape: Option<u8>,
}

fn dialect(
    context: &mut dyn Context,
    function: &str,
    args: &[Value],
) -> Result<Dialect, NativeError> {
    let separator = byte(function, args, 2, "separator", b',')?;
    let enclosure = byte(function, args, 3, "enclosure", b'"')?;
    let escape = match args::string(args, 4).map(|s| s.as_bytes()) {
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
                "{function}(): Argument #5 ($escape) must be empty or a single character"
            )));
        }
    };
    Ok(Dialect {
        separator,
        enclosure,
        escape,
    })
}

fn byte(
    function: &str,
    args: &[Value],
    index: usize,
    name: &str,
    default: u8,
) -> Result<u8, NativeError> {
    match args::string(args, index).map(|s| s.as_bytes()) {
        None => Ok(default),
        Some([byte]) => Ok(*byte),
        _ => Err(NativeError::value_error(format!(
            "{function}(): Argument #{} (${name}) must be a single character",
            index + 1
        ))),
    }
}

fn put(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = "fputcsv";
        let stream = stream_arg(&args, name)?;
        let dialect = dialect(context, name, &args)?;
        let mut line = Vec::new();
        if let Some(fields) = args::array(&args, 1) {
            for (index, value) in fields.values().enumerate() {
                if index > 0 {
                    line.push(dialect.separator);
                }
                let field = to_php_string(value, context.diagnostics())?;
                dialect.encode(field.as_bytes(), &mut line);
            }
        }
        line.extend_from_slice(args::string(&args, 5).map_or(b"\n", |string| string.as_bytes()));
        write(context, stream, &line, name).await
    })
}

fn get(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = "fgetcsv";
        let stream = stream_arg(&args, name)?;
        let dialect = dialect(context, name, &args)?;
        let length = args::int(&args, 1).unwrap_or(0);
        if !(0..i64::MAX).contains(&length) {
            return Err(NativeError::value_error(format!(
                "{name}(): Argument #2 ($length) must be between 0 and {}",
                i64::MAX - 1
            )));
        }
        let length = (length > 0).then_some(length as u64);
        let policy = with_stream(stream, |stream| stream.read_policy(None))?;
        let fields =
            match operate(stream, policy, move |stream| dialect.decode(stream, length)).await? {
                Ok(Some(fields)) => fields,
                Ok(None) => return Ok(Value::Bool(false)),
                Err(error) => {
                    read_notice(context, name, &error, read_amount(length));
                    return Ok(Value::Bool(false));
                }
            };
        let mut values = PhpArray::new();
        for field in fields {
            values.push(field.map_or(Value::Null, Value::from))?;
        }
        Ok(Value::from(values))
    })
}

impl Dialect {
    fn encode(self, field: &[u8], output: &mut Vec<u8>) {
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

    fn decode(
        self,
        stream: &mut Stream,
        length: Option<u64>,
    ) -> io::Result<Option<Vec<Option<Vec<u8>>>>> {
        let mut line = stream.line(length)?;
        if line.is_empty() {
            return Ok(None);
        }
        let mut end = line_end(&line);
        if end == 0 {
            return Ok(Some(vec![None]));
        }
        let mut fields = Vec::new();
        let mut offset = 0;
        loop {
            let mut field = Vec::new();
            let mut leading = offset;
            while leading < end && line[leading] != self.separator && space(line[leading]) {
                leading += 1;
            }
            if leading < end && line[leading] == self.enclosure {
                offset = leading + 1;
                loop {
                    if offset == end {
                        field.extend_from_slice(&line[end..]);
                        line = stream.line(None)?;
                        offset = 0;
                        end = line_end(&line);
                        if line.is_empty() {
                            break;
                        }
                        continue;
                    }
                    let byte = line[offset];
                    offset += 1;
                    if byte == self.enclosure {
                        if offset < end && line[offset] == self.enclosure {
                            offset += 1;
                        } else {
                            break;
                        }
                    } else if Some(byte) == self.escape && offset < end {
                        field.push(byte);
                        field.push(line[offset]);
                        offset += 1;
                        continue;
                    }
                    field.push(byte);
                }
            }
            let start = offset;
            while offset < end && line[offset] != self.separator {
                offset += 1;
            }
            field.extend_from_slice(&line[start..offset]);
            fields.push(Some(field));
            if offset == end {
                return Ok(Some(fields));
            }
            offset += 1;
        }
    }
}

fn line_end(line: &[u8]) -> usize {
    let trimmed = line.strip_suffix(b"\n").unwrap_or(line);
    trimmed.strip_suffix(b"\r").unwrap_or(trimmed).len()
}

fn space(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte == 11
}
