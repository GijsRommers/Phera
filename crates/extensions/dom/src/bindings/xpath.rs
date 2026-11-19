//! PHP's DOMXPath methods and namespace context.
//!
//! ```php
//! $xpath = new DOMXPath($document);
//! $xpath->registerNamespace('book', 'urn:library');
//! $titles = $xpath->query('//book:title');
//! ```

mod php;

use std::collections::HashMap;
use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue as C, Context, LocalFuture, MethodEntry as M,
    NativeClone, NativeError, NativePropertyAccess, NativePropertyMutation, NativePropertyRead,
    NativeResult, Object, Param as P, ParamType as T, Registry, Value, args,
};

use crate::tree::{Arena, Kind};
use crate::xpath::{self, callback::Failure};

use super::{This, collection, factory, location, native, node, receiver};

#[derive(Clone)]
struct State {
    document: Rc<Object>,
    auto: bool,
    namespaces: HashMap<String, String>,
    php: php::Handlers,
}

fn state(object: &Object) -> Result<State, NativeError> {
    native(object).ok_or_else(|| NativeError::error("DOMXPath object is uninitialized"))
}

fn construct(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let Some(document) = phera_extension_api::args::object(args, 0) else {
        return Err(NativeError::error("DOMXPath requires a document"));
    };
    location(document)?;
    *receiver(this)?.native.borrow_mut() = Some(Box::new(State {
        document: document.clone(),
        auto: phera_extension_api::args::bool(args, 1).unwrap_or(true),
        namespaces: HashMap::new(),
        php: php::Handlers::default(),
    }));
    Ok(Value::Null)
}

fn namespace(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let mut native = receiver(this)?.native.borrow_mut();
    let state = native
        .as_mut()
        .and_then(|data| data.downcast_mut::<State>())
        .ok_or_else(|| NativeError::error("DOMXPath object is uninitialized"))?;
    let prefix = factory::argument(args, 0).unwrap_or_default();
    let namespace = factory::argument(args, 1).unwrap_or_default();
    if prefix.is_empty() {
        return Ok(Value::Bool(false));
    }
    state.namespaces.insert(prefix, namespace);
    Ok(Value::Bool(true))
}

fn query<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    evaluate(context, this, args, true, "DOMXPath::query")
}

fn expression<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    evaluate(context, this, args, false, "DOMXPath::evaluate")
}

fn evaluate<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    nodes_only: bool,
    method: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(receiver(this.as_ref())?)?;
        let document = location(&state.document)?;
        let context_node = args::object(&arguments, 1)
            .map(|object| location(object))
            .transpose()?;
        if context_node
            .as_ref()
            .is_some_and(|node| !node.same_document(&document))
        {
            return Err(NativeError::error("Node from wrong document"));
        }
        let source = factory::argument(&arguments, 0).unwrap_or_default();
        let callbacks = source.contains(':') && source.contains('(');
        let mut namespaces = state.namespaces;
        namespaces.insert("xml".into(), crate::xml::names::XML.into());
        let (result, arena, id) = {
            let state_data = document.shared.borrow();
            let original = &state_data.document.arena;
            let id = context_node.as_ref().map(|node| node.id);
            if state.auto && args::bool(&arguments, 2).unwrap_or(true) {
                let element = id.or_else(|| {
                    original
                        .children(Arena::DOCUMENT)
                        .find(|&id| original[id].kind == Kind::Element)
                });
                if let Some(element) = element {
                    for (prefix, uri) in node::namespace_scope(original, element) {
                        if !prefix.is_empty() {
                            namespaces.entry(prefix).or_insert(uri);
                        }
                    }
                }
            }
            let id = id.unwrap_or(Arena::DOCUMENT);
            if callbacks {
                (None, Some(original.clone()), id)
            } else {
                (
                    Some(
                        xpath::evaluate(original, id, &namespaces, &source).map_err(Failure::from),
                    ),
                    None,
                    id,
                )
            }
        };
        let result = if let Some(arena) = arena {
            let mut handler = php::Invoke {
                arena: &arena,
                document: &document,
                namespaces: &namespaces,
                handlers: state.php,
                context,
                method,
            };
            xpath::callback::evaluate(&arena, id, &namespaces, &source, &mut handler).await
        } else {
            result.ok_or_else(|| NativeError::error("XPath evaluation could not start"))?
        };
        match result {
            Ok(xpath::Value::Nodes(nodes)) => collection::snapshot(context, document, nodes),
            Ok(_) if nodes_only => collection::snapshot(context, document, Vec::new()),
            Ok(xpath::Value::Number(value)) => Ok(Value::Float(value)),
            Ok(xpath::Value::String(value)) => Ok(Value::from(value)),
            Ok(xpath::Value::Boolean(value)) => Ok(Value::Bool(value)),
            Err(Failure::XPath(error)) => {
                context.warning(format!("{method}(): {}", error.message()));
                Ok(Value::Bool(false))
            }
            Err(Failure::Php(error)) => Err(error),
        }
    })
}

fn quote(_: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    Ok(Value::from(xpath::quote(
        &factory::argument(arguments, 0).unwrap_or_default(),
    )))
}

fn read(
    _: &mut dyn Context,
    object: &Object,
    name: &str,
    _: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    match name {
        "document" => Ok(Some(Value::Object(state(object)?.document))),
        "registerNodeNamespaces" => Ok(Some(Value::Bool(state(object)?.auto))),
        _ => Ok(None),
    }
}

fn mutate(
    _: &mut dyn Context,
    object: &Object,
    name: &str,
    mutation: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    match (name, mutation) {
        ("registerNodeNamespaces", NativePropertyMutation::Assign(value)) => {
            let mut native = object.native.borrow_mut();
            let state = native
                .as_mut()
                .and_then(|data| data.downcast_mut::<State>())
                .ok_or_else(|| NativeError::error("DOMXPath object is uninitialized"))?;
            state.auto = phera_extension_api::value::to_bool(value);
            Ok(true)
        }
        ("document" | "registerNodeNamespaces", _) => Err(NativeError::error(format!(
            "Cannot modify readonly property DOMXPath::${name}"
        ))),
        _ => Ok(false),
    }
}

pub(super) fn register(registry: &mut Registry) {
    const ARGS: &[P] = &[
        P::new("expression", T::String),
        P::new("contextNode", T::Class("DOMNode"))
            .nullable()
            .with_default(C::Null),
        P::new("registerNodeNS", T::Bool).with_default(C::Bool(true)),
    ];
    registry.class(ClassEntry {
        hooks: Some(&ClassHooks {
            property_access: Some(&NativePropertyAccess { read, mutate }),
            clone: Some(NativeClone::Forbidden),
            ..ClassHooks::EMPTY
        }),
        methods: const {
            &[
                M::public(
                    "__construct",
                    const {
                        &[
                            P::new("document", T::Class("DOMDocument")),
                            P::new("registerNodeNS", T::Bool).with_default(C::Bool(true)),
                        ]
                    },
                    construct,
                ),
                M::public_async("query", ARGS, query).tentative_return("mixed"),
                M::public_async("evaluate", ARGS, expression).tentative_return("mixed"),
                M::public(
                    "registerNamespace",
                    const { &[P::new("prefix", T::String), P::new("namespace", T::String)] },
                    namespace,
                )
                .tentative_return("bool"),
                M::public(
                    "registerPhpFunctions",
                    const {
                        &[P::new("restrict", T::ArrayOrString)
                            .nullable()
                            .with_default(C::Null)]
                    },
                    php::register,
                )
                .tentative_return("void"),
                M::public(
                    "registerPhpFunctionNS",
                    const {
                        &[
                            P::new("namespaceURI", T::String),
                            P::new("name", T::String),
                            P::new("callable", T::Callable),
                        ]
                    },
                    php::register_ns,
                )
                .returning("void"),
                M::public("quote", const { &[P::new("str", T::String)] }, quote)
                    .static_()
                    .returning("string"),
            ]
        },
        ..ClassEntry::class("DOMXPath")
    });
}
