//! Arguments that are paths, sent by value from the interpreter loop.
//!
//! ```php
//! $this->log($this->name);          // SendPath $this->name
//! format($row['title'], $options);  // SendPath $row['title']
//! ```
//!
//! The compiler sends a path as `SendPath` whenever it cannot know whether
//! the parameter takes a reference. When the prepared call says that it
//! takes a value (a PHP function's facts, or a described native callee),
//! the argument is the value the path holds, read as a read of it is: a
//! declared property through the instruction's property site
//! (`fast_properties`), an element of an array property
//! (`fast_property_elements`), or an element of a local array. Then it is
//! sent as `SendValue` sends it (`fast_calls`). A parameter that takes a
//! reference, a missing element (which warns), a PHP reference on the way
//! and anything the reads leave to the VM answer [`Step::Slow`] with
//! nothing changed.

use phera_bytecode::{PathBase, PathId, Step as PathStep};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::prepared_call::{CallFlags, PreparedCallee};

impl Machine<'_> {
    /// `SendPath` of a path whose value the callee takes.
    #[inline(never)]
    pub(super) fn send_path_value(&mut self, position: Position, path: PathId) -> Step {
        let Some(call) = self.stack.last_call().copied() else {
            return Step::Slow;
        };
        let index = call.sent as usize;
        let takes_value = match call.callee {
            PreparedCallee::Php(function) => {
                !call.flags.contains(CallFlags::DETAILS)
                    && self.code.facts(function).takes_value(index)
            }
            PreparedCallee::General | PreparedCallee::Native(_) => true,
            PreparedCallee::Constructed => false,
        };
        if !takes_value {
            return Step::Slow;
        }
        let Some(mut value) = self.path_value(position, path) else {
            return Step::Slow;
        };
        let accepted = match call.callee {
            PreparedCallee::Php(function) => {
                self.accepts_argument(function, index, &value)
                    || self.accepts_on_second_look(function, index, &mut value)
            }
            _ => self.other_accepts(call.callee, index, &value),
        };
        if !accepted {
            return Step::Slow;
        }
        let Some(call) = self.stack.last_call_mut() else {
            return Step::Slow;
        };
        self.registers.put(call.base() + index, value);
        call.mark_sent(index, true);
        Step::Done(position.ip)
    }

    /// The value `path` holds, when a plain read of it gives one.
    fn path_value(&self, position: Position, path: PathId) -> Option<Value> {
        let body = self
            .code
            .function(position.frame.function)
            .paths
            .get(path.index())?;
        match (body.base, body.steps.as_slice()) {
            (_, [PathStep::Property(_)]) => self.cached_property(position, path),
            (_, [PathStep::Property(_), PathStep::Index(_), ..]) => {
                self.property_element(position, path, false)
            }
            (PathBase::Register(_), [PathStep::Index(_), ..]) => {
                match self.local_element(position, path)?? {
                    Value::Reference(_) => None,
                    value => Some(value.clone()),
                }
            }
            _ => None,
        }
    }
}
