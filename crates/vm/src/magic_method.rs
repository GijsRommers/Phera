//! An engine operation can enter a linked method without applying user-call
//! visibility, for example `echo $object` running its __toString.

use std::rc::Rc;

use phera_extension_api::{LocalFuture, NativeResult};
use phera_value::{Object, Value};

use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee};
use crate::calls::callback_binding::{ArgumentBinding, ArgumentOrigin};
use crate::classes::{Class, Invocation, Method, MethodContext};

impl Vm {
    pub(crate) fn invoke_magic_method(
        &mut self,
        method: Rc<Method>,
        called: Rc<Class>,
        object: Rc<Object>,
        arguments: impl IntoIterator<Item = Value>,
    ) -> LocalFuture<'_, NativeResult> {
        let Some(scope) = self.classes.get(&method.declaring) else {
            return Box::pin(std::future::ready(Err(internal_error!(
                "magic method scope"
            ))));
        };
        let signature = method.signature(&self.code);
        let callee = Callee {
            name: signature.name.clone(),
            parameters: signature.parameters.clone(),
            argument_binding: ArgumentBinding::Caller,
            target: CallTarget::Method {
                method,
                context: MethodContext {
                    receiver: Some(object),
                    called,
                    scope,
                    invocation: Invocation::Method,
                },
            },
        };
        self.invoke_callable_origin(callee, arguments, ArgumentOrigin::Callback)
    }
}
