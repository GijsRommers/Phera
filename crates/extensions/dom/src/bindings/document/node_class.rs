//! Resolve registered PHP subclasses for future wrappers in a document.
//!
//! ```php
//! $document->registerNodeClass(DOMElement::class, CustomElement::class);
//! ```

use std::rc::Rc;

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value};

use super::super::{factory, this_node};

pub(super) fn register<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        const BASE: &str = "DOMDocument::registerNodeClass(): Argument #1 ($baseClass)";
        const SUBCLASS: &str = "DOMDocument::registerNodeClass(): Argument #2 ($extendedClass)";
        let node = this_node(this.as_ref())?;
        let name = factory::argument(&arguments, 0).unwrap_or_default();
        let base = context
            .lookup_class(&name)
            .await?
            .filter(|base| base.is_instance_of("DOMNode"));
        let Some(base) = base else {
            return Err(NativeError::type_error(format!(
                "{BASE} must be a class name derived from DOMNode, {name} given",
            )));
        };
        if let Some(name) = factory::argument(&arguments, 1) {
            let Some(class) = context.lookup_class(&name).await? else {
                return Err(NativeError::type_error(format!(
                    "{SUBCLASS} must be a valid class name or null, {name} given",
                )));
            };
            if !class.is_instance_of(base.name()) {
                return Err(NativeError::error(format!(
                    "{SUBCLASS} must be a class name derived from {} or null, {name} given",
                    base.name(),
                )));
            }
            if let Some(reflection) = context.reflection()
                && reflection.reflect_class(class.name()).await?.is_abstract
            {
                return Err(NativeError::value_error(format!(
                    "{SUBCLASS} must not be an abstract class",
                )));
            }
            node.shared
                .borrow_mut()
                .node_classes
                .insert(base.name().into(), class.name().into());
        } else {
            node.shared.borrow_mut().node_classes.remove(base.name());
        }
        Ok(Value::Bool(true))
    })
}
