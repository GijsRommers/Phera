//! PHP's DOMXPath methods and namespace context.
//!
//! ```php
//! $xpath = new DOMXPath($document);
//! $xpath->registerNamespace('book', 'urn:library');
//! $titles = $xpath->query('//book:title');
//! ```

mod modern;
mod php;
mod results;

pub(super) use modern::register_modern;

use std::collections::HashMap;
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ConstantValue as C, Context, LocalFuture, MethodEntry as M,
    NativeClone, NativeError, NativePropertyAccess, NativePropertyMutation, NativePropertyRead,
    NativeResult, Object, Param as P, ParamType as T, PhpArray, PropertyPurpose, Registry, Value,
    args,
};

use crate::tree::{Arena, Kind};
use crate::xpath::{self, callback::Failure};

use super::{This, collection, dom_error, factory, location, native, node, receiver};

#[derive(Clone)]
struct State {
    document: Rc<Object>,
    auto: bool,
    namespaces: HashMap<String, String>,
    php: php::Handlers,
}

fn state(object: &Object) -> Result<State, NativeError> {
    native(object).ok_or_else(|| dom_error(11, "Invalid State Error"))
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
        .ok_or_else(|| dom_error(11, "Invalid State Error"))?;
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
    let method = if this
        .as_ref()
        .is_some_and(|object| object.class.name() == "Dom\\XPath")
    {
        "Dom\\XPath::query"
    } else {
        "DOMXPath::query"
    };
    evaluate(context, this, args, true, method)
}

fn expression<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    let method = if this
        .as_ref()
        .is_some_and(|object| object.class.name() == "Dom\\XPath")
    {
        "Dom\\XPath::evaluate"
    } else {
        "DOMXPath::evaluate"
    };
    evaluate(context, this, args, false, method)
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
            let mut state_data = document.shared.borrow_mut();
            let original = &mut state_data.document.arena;
            let id = context_node.as_ref().map(|node| node.id);
            if state.auto && args::bool(&arguments, 2).unwrap_or(true) {
                let element = id.or_else(|| {
                    original
                        .children(Arena::DOCUMENT)
                        .find(|&id| original[id].kind == Kind::Element)
                });
                if let Some(element) = element {
                    let mut automatic = std::collections::HashSet::new();
                    for (prefix, uri) in node::namespace_scope(original, element) {
                        if !prefix.is_empty() && automatic.insert(prefix.clone()) {
                            namespaces.insert(prefix, uri);
                        }
                    }
                }
            }
            let id = id.unwrap_or(Arena::DOCUMENT);
            if callbacks {
                if xpath::uses_namespace_axis(&source) {
                    original.prepare_xpath_namespaces(id);
                }
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
        let mut located = None;
        let result = if let Some(mut arena) = arena {
            let mut handler = php::Invoke {
                document: &document,
                namespaces: &namespaces,
                handlers: state.php,
                context,
                method,
                nodes: results::Nodes::new(&arena),
            };
            let result =
                xpath::callback::evaluate(&mut arena, id, &namespaces, &source, &mut handler).await;
            if let Ok(xpath::Value::Nodes(ids)) = &result
                && handler.nodes.copies_used(ids)
            {
                located = Some(
                    ids.iter()
                        .map(|&id| handler.nodes.location(&document, id))
                        .collect(),
                );
            }
            result
        } else {
            result.ok_or_else(|| NativeError::error("XPath evaluation could not start"))?
        };
        match result {
            Ok(xpath::Value::Nodes(nodes)) => match located {
                Some(located) => collection::located(context, document, located),
                None => collection::snapshot(context, document, nodes),
            },
            Ok(_) if nodes_only => collection::snapshot(context, document, Vec::new()),
            Ok(xpath::Value::Number(value)) => Ok(Value::Float(value)),
            Ok(xpath::Value::String(value)) => Ok(Value::from(value)),
            Ok(xpath::Value::Boolean(value)) => Ok(Value::Bool(value)),
            Err(Failure::XPath(error)) => {
                context.warning(format!("{method}(): {}", error.message()));
                if method.starts_with("Dom\\") {
                    Err(NativeError::error("Could not evaluate XPath expression"))
                } else {
                    Ok(Value::Bool(false))
                }
            }
            Err(Failure::Php(error)) => Err(error),
        }
    })
}

fn quote(_: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let input = factory::argument(arguments, 0).unwrap_or_default();
    if input.contains('\0') {
        return Err(NativeError::value_error(
            "DOMXPath::quote(): Argument #1 ($str) must not contain any null bytes",
        ));
    }
    Ok(Value::from(xpath::quote(&input)))
}

fn debug(
    _: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let mut table = object.to_array();
    if purpose == PropertyPurpose::Debug {
        let state = native::<State>(object);
        if state.is_some() {
            table.insert(
                ArrayKey::str("document"),
                Value::from("(object value omitted)"),
            );
        }
        table.insert(
            ArrayKey::str("registerNodeNamespaces"),
            Value::Bool(state.is_none_or(|state| state.auto)),
        );
    }
    Ok(table)
}

fn read(
    _: &mut dyn Context,
    object: &Object,
    name: &str,
    _: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    match name {
        "document" => Ok(Some(Value::Object(state(object)?.document))),
        "registerNodeNamespaces" => Ok(Some(Value::Bool(
            native::<State>(object).is_none_or(|state| state.auto),
        ))),
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
                .ok_or_else(|| dom_error(11, "Invalid State Error"))?;
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
            properties: Some(debug),
            property_access: Some(&NativePropertyAccess {
                read,
                mutate,
                string_property: None,
            }),
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
