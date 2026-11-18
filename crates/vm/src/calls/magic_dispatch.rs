//! `$object->missing(1, second: 2)` calls __call with the written name and
//! an array that preserves positional and named arguments.

use std::rc::Rc;

use phera_extension_api::NativeError;
use phera_value::{ArrayKey, PhpArray, Value};

use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee};
use crate::calls::call_view::CallView;
use crate::classes::{Method, MethodContext};

/// A callable trampoline keeps the missing name independent of the handler.
#[derive(Clone)]
pub(crate) struct MagicCall {
    pub(crate) written: String,
    pub(crate) method: Rc<Method>,
    pub(crate) context: MethodContext,
}

impl Vm {
    /// Replace the trampoline window with an ordinary call on the heap frame
    /// stack. In particular, recursive __call dispatch uses PHP frame limits.
    pub(crate) fn expand_magic_call(&mut self) -> Result<(), NativeError> {
        let Some(Callee {
            target: CallTarget::Magic(_),
            ..
        }) = self
            .call_stack
            .top_details()
            .and_then(|details| details.callee.as_ref())
        else {
            return Ok(());
        };
        let slots = CallView::top(&self.code, &self.call_stack)?.slots();
        let (call, details) = self.call_stack.pop_call()?;
        let details = details.ok_or_else(|| internal_error!("magic call details"))?;
        let Some(Callee {
            argument_binding,
            target: CallTarget::Magic(magic),
            ..
        }) = details.callee
        else {
            return Err(internal_error!("magic call target"));
        };
        let mut named = details.extra_named.into_iter().peekable();
        let mut arguments = PhpArray::new();
        for index in 0..call.sent as usize {
            let value = self.registers.take(slots.slot(index));
            if named.peek().is_some_and(|(offset, _)| *offset == index) {
                if let Some((_, key)) = named.next() {
                    arguments.insert(ArrayKey::String(key), value);
                }
            } else {
                arguments.push(value)?;
            }
        }
        self.registers.truncate(call.start());
        let signature = magic.method.signature(&self.code);
        self.prepare_call(Callee {
            argument_binding,
            name: signature.name.clone(),
            parameters: signature.parameters.clone(),
            target: CallTarget::Method {
                method: magic.method.clone(),
                context: magic.context.clone(),
            },
        })?;
        self.store_sent_value(0, Value::from(magic.written.as_str()))?;
        self.store_sent_value(1, Value::Array(Rc::new(arguments)))
    }
}
