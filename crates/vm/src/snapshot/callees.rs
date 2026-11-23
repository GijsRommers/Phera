//! Callables in a fork: the same code, the copies of their objects.
//!
//! ```php
//! $mailer = new Mailer;
//! spl_autoload_register([$loader, 'load']);         // a bound method
//! set_error_handler(fn ($n, $s) => $mailer->log($s)); // a closure capturing $mailer
//! Phera\snapshot();
//! // each fork's autoloader calls its own copy of $loader, and its error
//! // handler's closure captures its own copy of $mailer
//! ```
//!
//! A resolved callable keeps the method or function it calls (immutable,
//! shared) and the objects it calls it on: the receiver, the closure
//! object, and for a closure its captured variables and static variables.
//! Only those objects and values are copied.

use std::cell::RefCell;
use std::rc::Rc;

use phera_value::Fork;

use super::ForkCopy;
use crate::calls::call_state::{CallTarget, Callee};
use crate::calls::magic_dispatch::MagicCall;
use crate::classes::MethodContext;
use crate::closures::ClosureState;

impl ForkCopy for MethodContext {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        MethodContext {
            receiver: self.receiver.fork_copy(fork),
            scope: self.scope.clone(),
            called: self.called.clone(),
            invocation: self.invocation,
        }
    }
}

impl ForkCopy for CallTarget {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        match self {
            CallTarget::Method { method, context } => CallTarget::Method {
                method: method.clone(),
                context: context.fork_copy(fork),
            },
            CallTarget::Closure(object) => CallTarget::Closure(fork.object(object)),
            CallTarget::Magic(call) => CallTarget::Magic(Rc::new(MagicCall {
                written: call.written.clone(),
                method: call.method.clone(),
                context: call.context.fork_copy(fork),
            })),
            CallTarget::User(_) | CallTarget::Native(_) | CallTarget::Intrinsic(_) => self.clone(),
        }
    }
}

impl Callee {
    /// This callee as a request forked from a snapshot may use it, if it
    /// may: one that holds no object (a function or an intrinsic).
    pub(crate) fn shared(&self) -> Option<Self> {
        match self.target {
            CallTarget::User(_) | CallTarget::Native(_) | CallTarget::Intrinsic(_) => {
                Some(self.clone())
            }
            CallTarget::Method { .. } | CallTarget::Closure(_) | CallTarget::Magic(_) => None,
        }
    }
}

impl ForkCopy for Callee {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        Callee {
            argument_binding: self.argument_binding,
            name: self.name.clone(),
            parameters: self.parameters.clone(),
            target: self.target.fork_copy(fork),
        }
    }
}

impl ForkCopy for ClosureState {
    fn fork_copy(&self, fork: &mut Fork<'_>) -> Self {
        let statics = self.statics.as_ref().map(|statics| {
            let copies = statics.borrow().fork_copy(fork);
            Rc::new(RefCell::new(copies))
        });
        ClosureState {
            origin: self.origin,
            callee: self.callee.fork_copy(fork),
            captures: self.captures.fork_copy(fork),
            context: self.context.fork_copy(fork),
            binding: self.binding,
            statics,
        }
    }
}
