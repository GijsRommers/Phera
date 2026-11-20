//! An object whose construction did not complete gets no destructor.
//!
//! ```php
//! class T { function __construct() { throw new Exception; } function __destruct() { echo "never\n"; } }
//! try { new T; } catch (Exception $e) {}
//! function boom() { throw new Exception; }
//! try { new T(boom()); } catch (Exception $e) {}   // nothing printed either
//! ```
//!
//! PHP allocates the object before it evaluates the constructor's
//! arguments. When an argument or the constructor itself throws, it marks
//! the object as already destructed (`zend_object_store_ctor_failed`), so
//! releasing it runs no destructor.

use phera_value::Value;

use crate::Vm;
use crate::calls::frame::Frame;
use crate::calls::prepared_call::{CallKind, PreparedCallee};

impl Vm {
    /// Drops the calls being prepared from register `base` up, as unwinding
    /// does; an object waiting for its constructor is marked destructed.
    pub(crate) fn abandon_calls_unconstructed(&mut self, base: usize) {
        while self.call_stack.has_calls_from(base) {
            let Ok((call, _)) = self.call_stack.pop_call() else {
                return;
            };
            if call.callee == PreparedCallee::Constructed || call.kind == CallKind::Constructor {
                self.mark_unconstructed(call.base().wrapping_sub(1));
            }
        }
    }

    /// A constructor frame that an exception leaves: its object was not
    /// constructed.
    pub(crate) fn mark_failed_constructor(&mut self, frame: &Frame) {
        if frame.kind() == CallKind::Constructor {
            self.mark_unconstructed(frame.base().wrapping_sub(1));
        }
    }

    fn mark_unconstructed(&mut self, register: usize) {
        if let [Some(Value::Object(object))] = self.registers.window(register, 1) {
            object.mark_destructed();
        }
    }
}
