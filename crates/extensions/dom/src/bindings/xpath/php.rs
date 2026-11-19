//! Invoke an explicitly registered PHP callback from an XPath expression.
//!
//! ```php
//! $xpath->registerPhpFunctionNS('urn:example', 'ready', fn () => true);
//! ```

use std::collections::HashMap;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, PhpArray, Value, args,
};

use crate::tree::Arena;
use crate::xml::names;
use crate::xpath::{
    Error, Value as XPath,
    callback::{Callback, Failure},
};

use super::super::{Location, This, dom_error, factory, location, receiver};
use super::State;

const PHP_NAMESPACE: &str = "http://php.net/xpath";
const RESTRICT: &str = "DOMXPath::registerPhpFunctions(): Argument #1 ($restrict)";

#[derive(Clone, Default)]
pub(super) struct Handlers {
    enabled: bool,
    unrestricted: bool,
    named: HashMap<String, Value>,
    namespaced: HashMap<(String, String), Value>,
}

pub(super) fn register(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let mut additions = HashMap::new();
    let unrestricted = args::value(arguments, 0).is_null();
    if let Some(array) = args::array(arguments, 0) {
        for (key, callable) in array.iter() {
            let callable = callable.deref();
            validate_registration(context, &callable, true)?;
            let name = match key {
                ArrayKey::String(name) => String::from_utf8_lossy(name.as_bytes()).into_owned(),
                _ => {
                    let name = phera_extension_api::value::to_php_string(
                        &callable,
                        context.diagnostics(),
                    )?;
                    String::from_utf8_lossy(name.as_bytes()).into_owned()
                }
            };
            if name.is_empty() || name.contains('\0') {
                return Err(NativeError::value_error(format!(
                    "{RESTRICT} must be an array containing valid callback names",
                )));
            }
            additions.insert(name, (*callable).clone());
        }
    } else if let Some(name) = factory::argument(arguments, 0) {
        if name.is_empty() || name.contains('\0') {
            return Err(NativeError::value_error(format!(
                "{RESTRICT} must be a valid callback name",
            )));
        }
        let callable = Value::from(name.as_str());
        validate_registration(context, &callable, false)?;
        additions.insert(name, callable);
    }
    let mut native = receiver(this)?.native.borrow_mut();
    let state = native
        .as_mut()
        .and_then(|data| data.downcast_mut::<State>())
        .ok_or_else(|| dom_error(11, "Invalid State Error"))?;
    state.php.enabled = true;
    state.php.unrestricted = unrestricted;
    state.php.named.extend(additions);
    Ok(Value::Null)
}

fn validate_registration(
    context: &mut dyn Context,
    callable: &Value,
    array: bool,
) -> Result<(), NativeError> {
    context
        .validate_callback("DOMXPath::registerPhpFunctions", 1, "restrict", callable)
        .map_err(|error| match error {
            NativeError::Throw {
                class: "TypeError",
                message,
                ..
            } => {
                let suffix = message
                    .split_once(", ")
                    .map_or(message.as_str(), |(_, reason)| reason);
                let expected = if array {
                    "an array with valid callbacks as values"
                } else {
                    "a callable"
                };
                NativeError::type_error(format!("{RESTRICT} must be {expected}, {suffix}"))
            }
            error => error,
        })
}

pub(super) fn register_ns(
    _: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let namespace = factory::argument(arguments, 0).unwrap_or_default();
    let name = factory::argument(arguments, 1).unwrap_or_default();
    const METHOD: &str = "DOMXPath::registerPhpFunctionNS()";
    for (index, parameter, value) in [(1, "namespaceURI", &namespace), (2, "name", &name)] {
        if value.contains('\0') {
            return Err(NativeError::value_error(format!(
                "{METHOD}: Argument #{index} (${parameter}) must not contain any null bytes",
            )));
        }
    }
    if namespace == PHP_NAMESPACE {
        return Err(NativeError::value_error(format!(
            "{METHOD}: Argument #1 ($namespaceURI) must not be \"{PHP_NAMESPACE}\" \
             because it is reserved by PHP",
        )));
    }
    if !names::valid(&name) || name.contains(':') {
        return Err(NativeError::value_error(format!(
            "{METHOD}: Argument #2 ($name) must be a valid callback name",
        )));
    }
    let mut native = receiver(this)?.native.borrow_mut();
    let state = native
        .as_mut()
        .and_then(|data| data.downcast_mut::<State>())
        .ok_or_else(|| dom_error(11, "Invalid State Error"))?;
    state
        .php
        .namespaced
        .insert((namespace, name), args::value(arguments, 2).clone());
    Ok(Value::Null)
}

pub(super) struct Invoke<'a> {
    pub(super) arena: &'a Arena,
    pub(super) document: &'a Location,
    pub(super) namespaces: &'a HashMap<String, String>,
    pub(super) handlers: Handlers,
    pub(super) context: &'a mut dyn Context,
    pub(super) method: &'static str,
}

impl Callback for Invoke<'_> {
    fn call<'a>(
        &'a mut self,
        name: &'a str,
        mut arguments: Vec<XPath>,
    ) -> LocalFuture<'a, Result<XPath, Failure>> {
        Box::pin(async move {
            let (prefix, name) = name.split_once(':').ok_or(Error::Function)?;
            let Some(namespace) = self.namespaces.get(prefix) else {
                self.context.warning(format!(
                    "{}(): xmlXPathCompOpEval: function {name} bound to undefined prefix {prefix}",
                    self.method,
                ));
                return Err(Failure::XPath(Error::Prefix));
            };
            let string_nodes = namespace == PHP_NAMESPACE && name == "functionString";
            let callback =
                if namespace == PHP_NAMESPACE && matches!(name, "function" | "functionString") {
                    if arguments.is_empty() {
                        return Err(Failure::Php(NativeError::error(
                            "Function name must be passed as the first argument",
                        )));
                    }
                    let XPath::String(name) = arguments.remove(0) else {
                        return Err(Failure::Php(NativeError::type_error(
                            "Handler name must be a string",
                        )));
                    };
                    if !self.handlers.enabled {
                        return Err(Failure::Php(NativeError::error(
                            "No callbacks were registered",
                        )));
                    }
                    let name = name.split('\0').next().unwrap_or_default();
                    if self.handlers.unrestricted {
                        Value::from(name)
                    } else {
                        self.handlers.named.get(name).cloned().ok_or_else(|| {
                            Failure::Php(NativeError::error(format!(
                                "No callback handler \"{name}\" registered"
                            )))
                        })?
                    }
                } else {
                    self.handlers
                        .namespaced
                        .get(&(namespace.clone(), name.into()))
                        .cloned()
                        .ok_or(Error::Function)?
                };
            let mut values = Vec::with_capacity(arguments.len());
            for argument in arguments {
                let value = match argument {
                    XPath::Nodes(nodes) if !string_nodes => {
                        let mut array = PhpArray::new();
                        for id in nodes {
                            let object =
                                super::super::namespace::wrap(self.context, self.document.at(id))
                                    .map_err(Failure::Php)?;
                            let _ = array.push(object);
                        }
                        Value::from(array)
                    }
                    XPath::Boolean(value) => Value::Bool(value),
                    XPath::Number(value) => Value::Float(value),
                    value => Value::from(value.string(self.arena)),
                };
                values.push(value);
            }
            let result = self
                .context
                .call(&callback, values)
                .await
                .map_err(Failure::Php)?;
            Ok(match result {
                Value::Bool(value) => XPath::Boolean(value),
                Value::Object(object) if object.class.is_instance_of("DOMNode") => {
                    let node = location(&object).map_err(Failure::Php)?;
                    if !node.same_document(self.document) || self.arena.get(node.id).is_none() {
                        return Err(Failure::Php(NativeError::error("Node from wrong document")));
                    }
                    XPath::Nodes(vec![node.id])
                }
                Value::Object(_) => {
                    return Err(Failure::Php(NativeError::type_error(
                        "Only objects that are instances of DOM nodes \
                         can be converted to an XPath expression",
                    )));
                }
                value => {
                    let string = self.context.stringify(&value).await.map_err(Failure::Php)?;
                    XPath::String(String::from_utf8_lossy(string.as_bytes()).into_owned())
                }
            })
        })
    }
}
