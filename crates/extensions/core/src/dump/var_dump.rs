use phera_extension_api::value::{PropertyScope, float_repr, split_property_key};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value,
};

use super::{Container, Place, Style, spaces, walk};

pub(crate) fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("value", ParamType::Mixed),
        Param::new("values", ParamType::Mixed).variadic(),
    ];
    registry.function(FunctionEntry::sync("var_dump", PARAMS, var_dump));
}

fn var_dump(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    for value in arguments {
        let mut out = Vec::new();
        walk(&mut VarDump, &mut out, value, context)?;
        context.write(&out);
    }
    Ok(Value::Null)
}

struct VarDump;

impl VarDump {
    /// Two spaces per level, then `&` for a shared reference.
    fn lead(out: &mut Vec<u8>, place: Place) {
        spaces(out, 2 * place.depth);
        if place.is_reference {
            out.push(b'&');
        }
    }
}

impl Style for VarDump {
    fn scalar(&mut self, out: &mut Vec<u8>, value: &Value, place: Place) {
        Self::lead(out, place);
        match value {
            Value::Bool(flag) => out.extend_from_slice(format!("bool({flag})").as_bytes()),
            Value::Int(int) => out.extend_from_slice(format!("int({int})").as_bytes()),
            Value::Float(float) => {
                out.extend_from_slice(format!("float({})", float_repr(*float)).as_bytes());
            }
            Value::String(string) => {
                out.extend_from_slice(format!("string({}) \"", string.len()).as_bytes());
                out.extend_from_slice(string.as_bytes());
                out.push(b'"');
            }
            Value::Resource(resource) => {
                let kind = if resource.is_closed() {
                    "Unknown"
                } else {
                    resource.kind
                };
                out.extend_from_slice(
                    format!("resource({}) of type ({kind})", resource.id).as_bytes(),
                );
            }
            // Arrays and objects are containers; a reference was followed.
            Value::Null | Value::Array(_) | Value::Object(_) | Value::Reference(_) => {
                out.extend_from_slice(b"NULL");
            }
        }
        out.push(b'\n');
    }

    fn open(&mut self, out: &mut Vec<u8>, container: &Container, len: usize, place: Place) {
        Self::lead(out, place);
        let head = match container {
            Container::Array(_) => format!("array({len}) {{\n"),
            Container::Object(object) => {
                format!("object({})#{} ({len}) {{\n", object.class.name(), object.id)
            }
        };
        out.extend_from_slice(head.as_bytes());
    }

    fn key(&mut self, out: &mut Vec<u8>, key: &ArrayKey, container: &Container, depth: usize) {
        spaces(out, 2 * depth);
        match (key, container) {
            (ArrayKey::Int(int), _) => out.extend_from_slice(format!("[{int}]").as_bytes()),
            (ArrayKey::String(string), Container::Array(_)) => {
                out.extend_from_slice(b"[\"");
                out.extend_from_slice(string.as_bytes());
                out.extend_from_slice(b"\"]");
            }
            // A property shows who may see it: ["name":protected] or
            // ["name":"Class":private].
            (ArrayKey::String(string), Container::Object(_)) => {
                let (name, scope) = split_property_key(string.as_bytes());
                out.extend_from_slice(b"[\"");
                out.extend_from_slice(name);
                out.push(b'"');
                match scope {
                    PropertyScope::Public => {}
                    PropertyScope::Protected => out.extend_from_slice(b":protected"),
                    PropertyScope::Private(class) => {
                        out.extend_from_slice(b":\"");
                        out.extend_from_slice(class);
                        out.extend_from_slice(b"\":private");
                    }
                }
                out.push(b']');
            }
        }
        out.extend_from_slice(b"=>\n");
    }

    fn after_entry(&mut self, _out: &mut Vec<u8>) {}

    fn close(&mut self, out: &mut Vec<u8>, _container: &Container, depth: usize) {
        spaces(out, 2 * depth);
        out.extend_from_slice(b"}\n");
    }

    fn recursion(
        &mut self,
        out: &mut Vec<u8>,
        _container: &Container,
        place: Place,
        _context: &mut dyn Context,
    ) {
        spaces(out, 2 * place.depth);
        out.extend_from_slice(b"*RECURSION*\n");
    }
}
