//! `var_dump(1.2345)` uses the same decimal precision setting as `serialize(1.2345)`.

use std::rc::Rc;

use phera_extension_api::value::{PropertyScope, float_repr_with_precision, split_property_key};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry, Value,
};

use super::{Container, Place, Style, spaces, walk};

pub(crate) fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("value", ParamType::Mixed),
        Param::new("values", ParamType::Mixed).variadic(),
    ];
    registry.function(FunctionEntry::asynchronous(
        "var_dump",
        PARAMS,
        dump::<false>,
    ));
    registry.function(FunctionEntry::asynchronous(
        "debug_zval_dump",
        PARAMS,
        dump::<true>,
    ));
    registry.describe_function("debug_zval_dump", "void", Some("standard"));
}

fn dump<const COUNTS: bool>(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        for value in &arguments {
            let mut out = Vec::new();
            walk(&mut VarDump::<COUNTS>(&arguments), &mut out, value, context).await?;
            context.write(&out);
        }
        Ok(Value::Null)
    })
}

struct VarDump<'a, const COUNTS: bool>(&'a [Value]);

impl<const COUNTS: bool> VarDump<'_, COUNTS> {
    /// Two spaces per level, then `&` for a shared reference.
    fn lead(out: &mut Vec<u8>, place: Place) {
        spaces(out, 2 * place.depth);
        if place.is_reference {
            out.push(b'&');
        }
    }
}

impl<const COUNTS: bool> Style for VarDump<'_, COUNTS> {
    fn call_site(&self) -> (&'static str, &[Value]) {
        let function = if COUNTS {
            "debug_zval_dump"
        } else {
            "var_dump"
        };
        (function, self.0)
    }

    fn before_debug(&mut self, out: &mut Vec<u8>, place: Place, context: &mut dyn Context) {
        Self::lead(out, place);
        context.write(out);
        out.clear();
    }

    fn enum_case(
        &mut self,
        out: &mut Vec<u8>,
        object: &phera_extension_api::Object,
        name: &phera_extension_api::PhpString,
        place: Place,
    ) -> bool {
        Self::lead(out, place);
        out.extend_from_slice(b"enum(");
        out.extend_from_slice(object.class.name().as_bytes());
        out.extend_from_slice(b"::");
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(b")\n");
        true
    }

    fn scalar(
        &mut self,
        out: &mut Vec<u8>,
        value: &Value,
        place: Place,
        context: &mut dyn Context,
    ) {
        Self::lead(out, place);
        match value {
            Value::Bool(flag) => out.extend_from_slice(format!("bool({flag})").as_bytes()),
            Value::Int(int) => out.extend_from_slice(format!("int({int})").as_bytes()),
            Value::Float(float) => {
                out.extend_from_slice(
                    format!(
                        "float({})",
                        float_repr_with_precision(
                            *float,
                            phera_extension_api::serialization::precision(context)
                        )
                    )
                    .as_bytes(),
                );
            }
            Value::String(string) => {
                out.extend_from_slice(format!("string({}) \"", string.len()).as_bytes());
                out.extend_from_slice(string.as_bytes());
                out.push(b'"');
                if COUNTS {
                    out.extend_from_slice(
                        format!(" refcount({})", string.storage_owners()).as_bytes(),
                    );
                }
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
        if !place.leading_written {
            Self::lead(out, place);
        }
        let head = match container {
            Container::Array(array) if COUNTS => {
                let count = Rc::strong_count(array).saturating_sub(1);
                format!("array({len}) refcount({count}){{\n")
            }
            Container::Array(_) => format!("array({len}) {{\n"),
            Container::Object(object) => {
                let class = object.class.name();
                let display = class.split('\0').next().unwrap_or(class);
                let prefix = match object.lazy().as_deref() {
                    Some(phera_extension_api::value::LazyObject::Pending {
                        strategy: phera_extension_api::value::LazyStrategy::Ghost,
                        ..
                    }) => "lazy ghost ",
                    Some(
                        phera_extension_api::value::LazyObject::Pending { .. }
                        | phera_extension_api::value::LazyObject::Proxy(_),
                    ) => "lazy proxy ",
                    _ => "",
                };
                if COUNTS {
                    let count = Rc::strong_count(object).saturating_sub(1);
                    format!(
                        "{prefix}object({display})#{} ({len}) refcount({count}){{\n",
                        object.id
                    )
                } else {
                    format!("{prefix}object({display})#{} ({len}) {{\n", object.id)
                }
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
                        out.extend_from_slice(
                            class.split(|byte| *byte == 0).next().unwrap_or(class),
                        );
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
        if !place.leading_written {
            spaces(out, 2 * place.depth);
        }
        out.extend_from_slice(b"*RECURSION*\n");
    }
}
