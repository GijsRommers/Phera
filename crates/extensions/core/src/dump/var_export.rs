use phera_extension_api::value::{float_repr, split_property_key};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry, Value,
    args,
};

use super::{Container, Place, PropertyView, Style, spaces, walk};

pub(crate) fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("value", ParamType::Mixed),
        Param::new("return", ParamType::Bool).optional(),
    ];
    registry.function(FunctionEntry::asynchronous(
        "var_export",
        PARAMS,
        var_export,
    ));
}

fn var_export(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(render(context, arguments))
}

async fn render(context: &mut dyn Context, arguments: Vec<Value>) -> NativeResult {
    let arguments = &arguments;
    let mut out = Vec::new();
    walk(&mut VarExport, &mut out, args::value(arguments, 0), context).await?;
    if args::bool(arguments, 1).unwrap_or(false) {
        Ok(Value::from(out))
    } else {
        context.write(&out);
        Ok(Value::Null)
    }
}

struct VarExport;

/// A string as PHP source: single-quoted, with NUL bytes spliced in as
/// double-quoted pieces because a single-quoted string cannot escape them.
fn quote(out: &mut Vec<u8>, bytes: &[u8]) {
    out.push(b'\'');
    for &byte in bytes {
        match byte {
            b'\'' => out.extend_from_slice(b"\\'"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            0 => out.extend_from_slice(b"' . \"\\0\" . '"),
            other => out.push(other),
        }
    }
    out.push(b'\'');
}

/// A float as PHP source. `1.0` must not read back as the int `1`.
fn float_source(float: f64) -> String {
    let mut text = float_repr(float);
    let looks_like_int = text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'-');
    if float.is_finite() && looks_like_int {
        text.push_str(".0");
    }
    text
}

impl Style for VarExport {
    fn properties(&self) -> PropertyView {
        PropertyView::Raw
    }

    fn scalar(&mut self, out: &mut Vec<u8>, value: &Value, _place: Place) {
        match value {
            Value::Bool(flag) => out.extend_from_slice(flag.to_string().as_bytes()),
            // The literal 9223372036854775808 is a float, so PHP writes the
            // smallest int as an expression.
            Value::Int(i64::MIN) => out.extend_from_slice(b"-9223372036854775807-1"),
            Value::Int(int) => out.extend_from_slice(int.to_string().as_bytes()),
            Value::Float(float) => out.extend_from_slice(float_source(*float).as_bytes()),
            Value::String(string) => quote(out, string.as_bytes()),
            // A resource has no source form.
            Value::Null
            | Value::Resource(_)
            | Value::Array(_)
            | Value::Object(_)
            | Value::Reference(_) => out.extend_from_slice(b"NULL"),
        }
    }

    fn open(&mut self, out: &mut Vec<u8>, container: &Container, _len: usize, place: Place) {
        if place.depth > 0 {
            out.push(b'\n');
            spaces(out, 2 * place.depth);
        }
        match container {
            Container::Array(_) => out.extend_from_slice(b"array (\n"),
            Container::Object(object) if object.class.name() == "stdClass" => {
                out.extend_from_slice(b"(object) array(\n");
            }
            Container::Object(object) => {
                out.extend_from_slice(
                    format!("\\{}::__set_state(array(\n", object.class.name()).as_bytes(),
                );
            }
        }
    }

    fn key(&mut self, out: &mut Vec<u8>, key: &ArrayKey, container: &Container, depth: usize) {
        // PHP indents the properties of an object one space further than
        // the elements of an array.
        let extra = usize::from(matches!(container, Container::Object(_)));
        spaces(out, 2 * depth + extra);
        match key {
            ArrayKey::Int(int) => out.extend_from_slice(int.to_string().as_bytes()),
            // Exported code names a property plainly, whatever its
            // visibility.
            ArrayKey::String(string) if extra == 1 => {
                quote(out, split_property_key(string.as_bytes()).0);
            }
            ArrayKey::String(string) => quote(out, string.as_bytes()),
        }
        out.extend_from_slice(b" => ");
    }

    fn after_entry(&mut self, out: &mut Vec<u8>) {
        out.extend_from_slice(b",\n");
    }

    fn close(&mut self, out: &mut Vec<u8>, container: &Container, depth: usize) {
        spaces(out, 2 * depth);
        match container {
            Container::Object(object) if object.class.name() != "stdClass" => {
                out.extend_from_slice(b"))");
            }
            _ => out.push(b')'),
        }
    }

    fn recursion(
        &mut self,
        out: &mut Vec<u8>,
        _container: &Container,
        _place: Place,
        context: &mut dyn Context,
    ) {
        context.warning("var_export does not handle circular references".to_string());
        out.extend_from_slice(b"NULL");
    }
}
