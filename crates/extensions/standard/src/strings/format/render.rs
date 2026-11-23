//! Rendering and writing complete formats. `printf("%s", $value)` waits
//! for `__toString` only when the format actually uses that object.

use phera_extension_api::{Context, NativeError, Value, value::to_php_string};

use super::{Values, parser, value::Specifier};
use crate::strings::buffer;

/// The whole result is built before anything is written, so `printf`
/// writes nothing when a later specifier fails.
pub async fn render(
    context: &mut dyn Context,
    format: &[u8],
    values: &[Value],
    function: &str,
    source: Values,
) -> Result<Vec<u8>, NativeError> {
    let mut formatter = Formatter::new(format, values)?;
    while let Some((specifier, value)) = formatter.advance(context, function, source)? {
        let string = context.stringify(value).await?;
        specifier.append_string(&mut formatter.result, string.as_bytes())?;
    }
    Ok(formatter.result)
}

pub(super) struct Formatter<'a> {
    parser: parser::Parser<'a>,
    pub(super) result: Vec<u8>,
}

impl<'a> Formatter<'a> {
    pub(super) fn new(format: &'a [u8], values: &'a [Value]) -> Result<Self, NativeError> {
        Ok(Self {
            parser: parser::Parser::new(format, values),
            result: buffer(format.len())?,
        })
    }

    /// Format until the end or until a string conversion needs to call PHP.
    pub(super) fn advance(
        &mut self,
        context: &mut dyn Context,
        function: &str,
        source: Values,
    ) -> Result<Option<(Specifier, &'a Value)>, NativeError> {
        let parser = &mut self.parser;
        let format = parser.format;
        let result = &mut self.result;
        while let Some(&byte) = format.get(parser.cursor) {
            parser.cursor += 1;
            if byte != b'%' {
                result.push(byte);
                continue;
            }
            if format.get(parser.cursor) == Some(&b'%') {
                result.push(b'%');
                parser.cursor += 1;
                continue;
            }
            let Some((specifier, value)) = parser.specifier()? else {
                continue;
            };
            if specifier.conversion == b's' {
                match value {
                    Value::String(string) => specifier.append_string(result, string.as_bytes())?,
                    _ if value.is_object() => return Ok(Some((specifier, value))),
                    _ => {
                        let string = to_php_string(value, context.diagnostics())?;
                        specifier.append_string(result, string.as_bytes())?;
                    }
                }
            } else if let Some(text) = specifier.convert(context, value, function, result)? {
                specifier.append(result, &text)?;
            }
        }
        if parser.needed > 0 {
            return Err(too_few_values(source, parser.needed, parser.values.len()));
        }
        Ok(None)
    }
}

/// PHP's error when the format refers to more values than there are. The
/// argument forms count the format as an argument too.
fn too_few_values(source: Values, needed: usize, given: usize) -> NativeError {
    match source {
        Values::Array => NativeError::value_error(format!(
            "The arguments array must contain {needed} items, {given} given"
        )),
        Values::Arguments | Values::StreamArguments => {
            let preceding = if source == Values::StreamArguments {
                2
            } else {
                1
            };
            NativeError::argument_count_error(format!(
                "{} arguments are required, {} given",
                needed + preceding,
                given + preceding,
            ))
        }
    }
}
