//! One iterative unwind routine crosses PHP frames and native callback boundaries.
//!
//! `try { invoke(); } catch (Error $error) { recover(); }` has no try-entry
//! instruction. Its table covers invoke's instructions and names the catch
//! tests. A throw scans the innermost handler in each frame; frames without
//! one release their registers, iterators and incomplete argument lists. The
//! surviving frame receives the same object in a reserved register and resumes
//! its instanceof tests. At a native callback boundary the object travels in
//! NativeError::Unwind until the outer interpreter resumes this same algorithm.

use crate::{
    Vm,
    exceptions::{Thrown, throwable_root},
    finally::Continuation,
};
use phera_bytecode::{Handler, Instruction};
use phera_extension_api::NativeError;
use phera_value::{Object, Value};
use std::rc::Rc;

impl Vm {
    /// Only error exits consult tables; normal instruction dispatch remains unchanged.
    pub(crate) fn handle_exception(
        &mut self,
        error: NativeError,
        stop: usize,
    ) -> Result<(), NativeError> {
        let error = self.materialize_error(error);
        let NativeError::Unwind(payload) = error else {
            return Err(error);
        };
        let thrown = payload
            .0
            .downcast::<Thrown>()
            .map_err(|payload| NativeError::Unwind(phera_extension_api::Unwind(payload)))?;
        let object = thrown.object;
        let mut skip_handlers = thrown.skip_handlers;
        while self.call_stack.depth() > stop {
            let handler = self.current().and_then(|frame| {
                let position = frame.ip().saturating_sub(1);
                // Parameter checks occur before the body and cannot enter its catches.
                if frame.ip() == 0 || skip_handlers == Some(self.call_stack.depth()) {
                    return None;
                }
                frame
                    .function()
                    .handlers
                    .iter()
                    .find(|handler| {
                        handler.start.index() <= position && position < handler.end.index()
                    })
                    .cloned()
            });
            let target = handler
                .as_ref()
                .and_then(|handler| handler.catch.or(handler.finally));
            self.replace_finally_exceptions(&object, target.map(|target| target.index()));
            if let Some(handler) = handler {
                self.prepare_handler(&handler);
                if let Some(target) = handler.catch {
                    self.set(handler.exception, Value::Object(object));
                    return self.jump(target);
                }
                if let Some(target) = handler.finally {
                    return self.enter_finally(
                        handler.exception,
                        target,
                        Continuation::Throw { object },
                    );
                }
            }
            if skip_handlers == Some(self.call_stack.depth()) {
                skip_handlers = None;
            }
            self.discard_exception_frame()?;
        }
        Err(Thrown {
            object,
            skip_handlers,
        }
        .into_error())
    }

    /// Abandoned calls and try-local expression owners cannot survive into a catch.
    fn prepare_handler(&mut self, handler: &Handler) {
        let target = handler
            .catch
            .or(handler.finally)
            .map_or(0, |target| target.index());
        let Some(frame) = self.current() else {
            return;
        };
        let (base, end) = (frame.base(), frame.end());
        let iterators: Vec<_> = frame
            .function()
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(start, instruction)| {
                let (iterator, end) = match instruction {
                    Instruction::IterNext { iter, end, .. }
                    | Instruction::IterNextKeyed { iter, end, .. } => (*iter, *end),
                    _ => return None,
                };
                (!(start <= target && target < end.index())).then_some(iterator)
            })
            .collect();
        self.call_stack.abandon_calls_from(end);
        self.closure_captures.retain(|capture| capture.base < base);
        self.registers.truncate(end);
        for register in &handler.cleanup {
            self.registers
                .window_mut(base + register.index(), 1)
                .fill(None);
        }
        if !iterators.is_empty() {
            let extras = self.call_stack.current_extras_mut();
            for iterator in iterators {
                extras.iterators.remove(&iterator.0);
            }
        }
        let depth =
            self.frames()
                .rev()
                .enumerate()
                .map(|(index, frame)| {
                    let end = if index == 0 { target } else { frame.ip() };
                    frame.function().instructions.iter().take(end).fold(
                        0usize,
                        |depth, instruction| match instruction {
                            Instruction::BeginSilence => depth + 1,
                            Instruction::EndSilence => depth.saturating_sub(1),
                            _ => depth,
                        },
                    )
                })
                .sum();
        self.diagnostic_reporter.restore_silence(depth);
    }

    /// Include symbol-table changes survive exceptional exit just as normal return.
    fn discard_exception_frame(&mut self) -> Result<(), NativeError> {
        self.finish_include_scope()?;
        self.abandon_native_view();
        if let Some(frame) = self.call_stack.pop() {
            self.closure_captures
                .retain(|capture| capture.base < frame.base());
            self.registers.truncate(frame.start());
            self.call_stack.abandon_calls_from(frame.start());
        }
        Ok(())
    }

    /// A throw escaping finally appends its saved exception to the new previous chain.
    fn replace_finally_exceptions(&mut self, object: &Rc<Object>, target: Option<usize>) {
        let Some(frame_base) = self.current().map(|frame| frame.base()) else {
            return;
        };
        let mut index = self.finally_stack.len();
        while index > 0 {
            index -= 1;
            let saved = &self.finally_stack[index];
            if saved.frame_base != frame_base
                || target.is_some_and(|target| saved.start <= target && target <= saved.end)
            {
                continue;
            }
            if let Continuation::Throw { object: previous } =
                self.finally_stack.remove(index).continuation
            {
                append_previous(object, previous);
            }
        }
    }
}

/// Append at the tail, avoiding self-links and cycles as Zend does on replacement throws.
fn append_previous(object: &Rc<Object>, previous: Rc<Object>) {
    let mut ancestor = Some(previous.clone());
    let mut ancestors = Vec::new();
    while let Some(candidate) = ancestor {
        if Rc::ptr_eq(&candidate, object)
            || ancestors.iter().any(|seen| Rc::ptr_eq(seen, &candidate))
        {
            return;
        }
        ancestors.push(candidate.clone());
        ancestor = match candidate.get(&Object::private_key(throwable_root(&candidate), "previous"))
        {
            Some(Value::Object(parent)) => Some(parent),
            _ => None,
        };
    }
    let mut current = object.clone();
    let mut seen = Vec::new();
    loop {
        if Rc::ptr_eq(&current, &previous)
            || ancestors
                .iter()
                .any(|ancestor| Rc::ptr_eq(ancestor, &current))
            || seen.iter().any(|seen| Rc::ptr_eq(seen, &current))
        {
            return;
        }
        seen.push(current.clone());
        let key = Object::private_key(throwable_root(&current), "previous");
        if let Some(Value::Object(next)) = current.get(&key) {
            current = next;
        } else {
            current.set(key, Value::Object(previous));
            return;
        }
    }
}
