//! An engine operation calls the public method selected by the receiver's class.
//!
//! ```php
//! count($items); // calls the implementation of Countable::count()
//! ```

use std::rc::Rc;

use phera_extension_api::NativeError;
use phera_value::Object;

use crate::{
    Vm,
    calls::{
        call_state::{CallTarget, Callee},
        callback_binding::ArgumentBinding,
    },
    classes::{Invocation, MethodContext},
};

impl Vm {
    pub(crate) fn protocol_method(
        &self,
        object: Rc<Object>,
        name: &str,
    ) -> Result<Callee, NativeError> {
        self.protocol_method_in_scope(object, name, phera_bytecode::Visibility::Public)
    }

    pub(crate) fn protocol_method_in_scope(
        &self,
        object: Rc<Object>,
        name: &str,
        visibility: phera_bytecode::Visibility,
    ) -> Result<Callee, NativeError> {
        let class = self
            .classes
            .for_object(&object)
            .ok_or_else(|| internal_error!("protocol receiver class"))?;
        let method = class
            .methods
            .get(&name.to_ascii_lowercase())
            .filter(|method| {
                !method.is_abstract
                    && (method.visibility == phera_bytecode::Visibility::Public
                        || method.visibility == visibility)
            })
            .cloned()
            .ok_or_else(|| {
                NativeError::error(format!("Call to undefined method {}::{name}()", class.name))
            })?;
        let scope = self
            .classes
            .get(&method.declaring)
            .ok_or_else(|| internal_error!("protocol method scope"))?;
        let signature = method.signature(&self.code);
        Ok(Callee {
            argument_binding: ArgumentBinding::Internal,
            name: signature.name.clone(),
            parameters: signature.parameters.clone(),
            target: CallTarget::Method {
                method,
                context: MethodContext {
                    scope,
                    called: class,
                    receiver: Some(object),
                    invocation: Invocation::Method,
                },
            },
        })
    }
}
