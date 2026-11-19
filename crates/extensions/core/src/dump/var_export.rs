//! `var_export(1.0)` preserves the float type and honors `serialize_precision`.

use std::rc::Rc;

use phera_extension_api::value::{float_repr_with_precision, split_property_key};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray,
    PropertyPurpose, PropertyViewContinuation, PropertyViewRequest, Registry, Value, args,
};

use super::walker::{WalkView, Walker};
use super::{Container, Place, Style, spaces};

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
    Box::pin(render(
        context,
        Export {
            walker: Walker::default(),
            out: Vec::new(),
            arguments,
        },
        true,
    ))
}

struct Export {
    walker: Walker,
    out: Vec<u8>,
    arguments: Vec<Value>,
}

struct PendingExport {
    export: Export,
    view: WalkView,
}

impl PropertyViewContinuation for PendingExport {
    fn resume(
        self: Box<Self>,
        context: &mut dyn Context,
        properties: Rc<PhpArray>,
    ) -> LocalFuture<'_, NativeResult> {
        let Self { mut export, view } = *self;
        export
            .walker
            .resume(&mut VarExport, &mut export.out, view, properties);
        Box::pin(render(context, export, false))
    }
}

async fn render(context: &mut dyn Context, mut export: Export, start: bool) -> NativeResult {
    let root = start.then(|| args::value(&export.arguments, 0));
    if let Some(view) = export
        .walker
        .advance(&mut VarExport, &mut export.out, root, context)
        .await?
    {
        return Err(PropertyViewRequest {
            object: view.object.clone(),
            purpose: PropertyPurpose::Export,
            function: "var_export",
            arguments: Vec::new(),
            continuation: Box::new(PendingExport { export, view }),
        }
        .suspend());
    }
    if args::bool(&export.arguments, 1).unwrap_or(false) {
        Ok(Value::from(export.out))
    } else {
        context.write(&export.out);
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
fn float_source(float: f64, precision: i32) -> String {
    let mut text = float_repr_with_precision(float, precision);
    let looks_like_int = text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'-');
    if float.is_finite() && looks_like_int {
        text.push_str(".0");
    }
    text
}

impl Style for VarExport {
    fn call_site(&self) -> (&'static str, &[Value]) {
        ("var_export", &[])
    }

    fn property_purpose(&self) -> phera_extension_api::PropertyPurpose {
        phera_extension_api::PropertyPurpose::Export
    }

    fn enum_case(
        &mut self,
        out: &mut Vec<u8>,
        object: &phera_extension_api::Object,
        name: &phera_extension_api::PhpString,
        place: Place,
    ) -> bool {
        if place.depth > 0 {
            out.push(b'\n');
            spaces(out, 2 * place.depth);
        }
        out.push(b'\\');
        out.extend_from_slice(object.class.name().as_bytes());
        out.extend_from_slice(b"::");
        out.extend_from_slice(name.as_bytes());
        true
    }

    fn scalar(
        &mut self,
        out: &mut Vec<u8>,
        value: &Value,
        _place: Place,
        context: &mut dyn Context,
    ) {
        match value {
            Value::Bool(flag) => out.extend_from_slice(flag.to_string().as_bytes()),
            // The literal 9223372036854775808 is a float, so PHP writes the
            // smallest int as an expression.
            Value::Int(i64::MIN) => out.extend_from_slice(b"-9223372036854775807-1"),
            Value::Int(int) => out.extend_from_slice(int.to_string().as_bytes()),
            Value::Float(float) => out.extend_from_slice(
                float_source(
                    *float,
                    phera_extension_api::serialization::precision(context),
                )
                .as_bytes(),
            ),
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
