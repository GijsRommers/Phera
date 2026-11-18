use phera_extension_api::value::{NoDiagnostics, PropertyScope, split_property_key, to_php_string};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry, Value,
    args,
};

use super::{Container, Place, Style, spaces, walk};

pub(crate) fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("value", ParamType::Mixed),
        Param::new("return", ParamType::Bool).optional(),
    ];
    registry.function(FunctionEntry::asynchronous("print_r", PARAMS, print_r));
}

fn print_r(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(render(context, arguments))
}

async fn render(context: &mut dyn Context, arguments: Vec<Value>) -> NativeResult {
    let arguments = &arguments;
    let mut out = Vec::new();
    walk(&mut PrintR, &mut out, args::value(arguments, 0), context).await?;
    if args::bool(arguments, 1).unwrap_or(false) {
        Ok(Value::from(out))
    } else {
        context.write(&out);
        Ok(Value::Bool(true))
    }
}

struct PrintR;

impl PrintR {
    fn head(container: &Container) -> String {
        match container {
            Container::Array(_) => "Array\n".to_string(),
            Container::Object(object) => format!("{} Object\n", object.class.name()),
        }
    }
}

impl Style for PrintR {
    fn scalar(&mut self, out: &mut Vec<u8>, value: &Value, _place: Place) {
        // Scalars print as their string form, without the warnings a real
        // conversion would raise.
        if let Ok(text) = to_php_string(value, &mut NoDiagnostics) {
            out.extend_from_slice(text.as_bytes());
        }
    }

    fn open(&mut self, out: &mut Vec<u8>, container: &Container, _len: usize, place: Place) {
        out.extend_from_slice(Self::head(container).as_bytes());
        spaces(out, 8 * place.depth);
        out.extend_from_slice(b"(\n");
    }

    fn key(&mut self, out: &mut Vec<u8>, key: &ArrayKey, container: &Container, depth: usize) {
        spaces(out, 8 * depth - 4);
        out.push(b'[');
        match (key, container) {
            (ArrayKey::Int(int), _) => out.extend_from_slice(int.to_string().as_bytes()),
            (ArrayKey::String(string), Container::Array(_)) => {
                out.extend_from_slice(string.as_bytes());
            }
            // A property shows who may see it: [name:protected] or
            // [name:Class:private].
            (ArrayKey::String(string), Container::Object(_)) => {
                let (name, scope) = split_property_key(string.as_bytes());
                out.extend_from_slice(name);
                match scope {
                    PropertyScope::Public => {}
                    PropertyScope::Protected => out.extend_from_slice(b":protected"),
                    PropertyScope::Private(class) => {
                        out.push(b':');
                        out.extend_from_slice(class);
                        out.extend_from_slice(b":private");
                    }
                }
            }
        }
        out.extend_from_slice(b"] => ");
    }

    fn after_entry(&mut self, out: &mut Vec<u8>) {
        out.push(b'\n');
    }

    fn close(&mut self, out: &mut Vec<u8>, _container: &Container, depth: usize) {
        spaces(out, 8 * depth);
        out.extend_from_slice(b")\n");
    }

    fn recursion(
        &mut self,
        out: &mut Vec<u8>,
        container: &Container,
        _place: Place,
        _context: &mut dyn Context,
    ) {
        out.extend_from_slice(Self::head(container).as_bytes());
        out.extend_from_slice(b" *RECURSION*");
    }
}
