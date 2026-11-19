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
use crate::xpath::{
    Error, Value as XPath,
    callback::{Callback, Failure},
};

use super::super::{Location, This, factory, location, receiver};
use super::State;

const PHP_NAMESPACE: &str = "http://php.net/xpath";

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
            let name = match key {
                ArrayKey::String(name) => String::from_utf8_lossy(name.as_bytes()).into_owned(),
                _ => match callable {
                    Value::String(name) => String::from_utf8_lossy(name.as_bytes()).into_owned(),
                    _ => return Err(NativeError::type_error("Callback name must be a string")),
                },
            };
            context.validate_callback("DOMXPath::registerPhpFunctions", 1, "restrict", callable)?;
            additions.insert(name, callable.clone());
        }
    } else if let Some(name) = factory::argument(arguments, 0) {
        let callable = Value::from(name.as_str());
        context.validate_callback("DOMXPath::registerPhpFunctions", 1, "restrict", &callable)?;
        additions.insert(name, callable);
    }
    let mut native = receiver(this)?.native.borrow_mut();
    let state = native
        .as_mut()
        .and_then(|data| data.downcast_mut::<State>())
        .ok_or_else(|| NativeError::error("DOMXPath object is uninitialized"))?;
    state.php.enabled = true;
    state.php.unrestricted = unrestricted;
    state.php.named.extend(additions);
    Ok(Value::Null)
}

pub(super) fn register_ns(
    _: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let namespace = factory::argument(arguments, 0).unwrap_or_default();
    let name = factory::argument(arguments, 1).unwrap_or_default();
    let mut native = receiver(this)?.native.borrow_mut();
    let state = native
        .as_mut()
        .and_then(|data| data.downcast_mut::<State>())
        .ok_or_else(|| NativeError::error("DOMXPath object is uninitialized"))?;
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
                    if !self.handlers.enabled {
                        return Err(Failure::Php(NativeError::error(
                            "PHP functions are not registered",
                        )));
                    }
                    if arguments.is_empty() {
                        return Err(Error::Arity.into());
                    }
                    let XPath::String(name) = arguments.remove(0) else {
                        return Err(Error::Type.into());
                    };
                    if self.handlers.unrestricted {
                        Value::from(name)
                    } else {
                        self.handlers.named.get(&name).cloned().ok_or_else(|| {
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
                            let object = self
                                .document
                                .at(id)
                                .wrap(self.context)
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
                value => {
                    let string = self.context.stringify(&value).await.map_err(Failure::Php)?;
                    XPath::String(String::from_utf8_lossy(string.as_bytes()).into_owned())
                }
            })
        })
    }
}
