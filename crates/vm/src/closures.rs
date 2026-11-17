//! Native closure objects retain lexical bindings: `$f = function () use (&$x) { ++$x; };`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use phera_bytecode::{CaptureKind, ClosureBinding, FunctionId, Reg};
use phera_extension_api::NativeError;
use phera_value::{Object, Value};

use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee, ParameterSpec};
use crate::classes::MethodContext;

/// A pending capture belongs to a frame, so throwing from its warning handler releases it.
pub(crate) struct PendingCapture {
    pub(crate) base: usize,
    pub(crate) value: Option<Value>,
}

/// Lexical bodies own captures; callable wrappers preserve the original function's static storage.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClosureOrigin {
    Lexical,
    Callable,
}

/// Each object owns its static cells independently of other evaluations of the same expression.
pub(crate) struct ClosureState {
    pub(crate) origin: ClosureOrigin,
    pub(crate) callee: Callee,
    pub(crate) captures: Vec<Option<Value>>,
    pub(crate) context: Option<MethodContext>,
    pub(crate) binding: ClosureBinding,
    pub(crate) statics: Rc<RefCell<HashMap<u16, Value>>>,
}

impl Vm {
    /// Explicit uses copy values now; references create the cell even before its first assignment.
    pub(crate) fn capture(&mut self, source: Reg, kind: CaptureKind) -> Result<(), NativeError> {
        let value = match kind {
            CaptureKind::Value => Some(self.read_local(source)),
            CaptureKind::Quiet => self
                .current()
                .and_then(|frame| frame.slots(&self.registers).get(source.index()))
                .and_then(Option::as_ref)
                .map(|value| value.deref().clone()),
            CaptureKind::Reference => {
                let frame = self
                    .call_stack
                    .current_mut()
                    .ok_or_else(|| internal_error!("capture frame"))?;
                let slot = frame
                    .slots_mut(&mut self.registers)
                    .get_mut(source.index())
                    .ok_or_else(|| internal_error!("capture slot"))?;
                if !slot.as_ref().is_some_and(Value::is_reference) {
                    let value = slot.take().unwrap_or_default();
                    *slot = Some(Value::Reference(Rc::new(RefCell::new(value))));
                }
                slot.clone()
            }
        };
        let base = self
            .current()
            .ok_or_else(|| internal_error!("capture frame"))?
            .base;
        self.closure_captures.push(PendingCapture { base, value });
        Ok(())
    }

    /// Code and signature are immutable; only the captured PHP values belong to this request.
    pub(crate) fn make_closure(
        &mut self,
        destination: Reg,
        function: FunctionId,
        binding: ClosureBinding,
    ) -> Result<(), NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("closure frame"))?;
        let unit = frame.unit.clone();
        let body = unit
            .functions
            .get(function.index())
            .ok_or_else(|| internal_error!("closure body"))?;
        let capture_count = body.captures as usize;
        let context = self.closure_context(binding);
        let callee = closure_callee(unit, function)?;
        let first_capture = self
            .closure_captures
            .len()
            .checked_sub(capture_count)
            .ok_or_else(|| internal_error!("capture count"))?;
        let captures = self
            .closure_captures
            .split_off(first_capture)
            .into_iter()
            .map(|capture| capture.value)
            .collect();
        let state = ClosureState {
            origin: ClosureOrigin::Lexical,
            callee,
            context,
            binding,
            captures,
            statics: Rc::new(RefCell::new(HashMap::new())),
        };
        let object = self.allocate_closure(state)?;
        self.set(destination, Value::Object(object));
        Ok(())
    }

    /// A static closure keeps private class access while deliberately omitting the receiver.
    fn closure_context(&self, binding: ClosureBinding) -> Option<MethodContext> {
        let mut context = self.current()?.method.as_ref()?.clone();
        context.invocation = crate::classes::Invocation::Method;
        if binding == ClosureBinding::Static {
            context.receiver = None;
        }
        Some(context)
    }

    /// Native payload ownership gives each closure independent captures and static cells.
    pub(crate) fn allocate_closure(
        &mut self,
        state: ClosureState,
    ) -> Result<Rc<Object>, NativeError> {
        let class = self
            .classes
            .get("Closure")
            .ok_or_else(|| internal_error!("Closure class"))?;
        let object = self.classes.allocate(class);
        *object.native.borrow_mut() = Some(Box::new(state));
        Ok(object)
    }
}

impl ClosureState {
    /// PHP clone and bind copy the current static values into independent reference cells.
    pub(crate) fn duplicate(&self) -> Self {
        let statics = self
            .statics
            .borrow()
            .iter()
            .map(|(key, value)| {
                let cloned = match value {
                    Value::Reference(reference) if Rc::strong_count(reference) > 1 => value.clone(),
                    _ => Value::Reference(Rc::new(RefCell::new(value.deref().clone()))),
                };
                (*key, cloned)
            })
            .collect();
        Self {
            origin: self.origin,
            callee: self.callee.clone(),
            captures: self.captures.clone(),
            context: self.context.clone(),
            binding: self.binding,
            statics: Rc::new(RefCell::new(statics)),
        }
    }
}

impl Vm {
    /// Function-derived closures share the original named function's statics.
    pub(crate) fn lexical_closure(&self) -> Option<Rc<Object>> {
        let object = self.current()?.closure.as_ref()?;
        let lexical = object
            .native
            .borrow()
            .as_ref()
            .and_then(|state| state.downcast_ref::<ClosureState>())
            .is_some_and(|state| state.origin == ClosureOrigin::Lexical);
        lexical.then(|| object.clone())
    }
}

/// Captured values remain per object; the signature only holds immutable unit metadata.
fn closure_callee(
    unit: std::sync::Arc<phera_bytecode::CompiledUnit>,
    function: FunctionId,
) -> Result<Callee, NativeError> {
    let body = unit
        .functions
        .get(function.index())
        .ok_or_else(|| internal_error!("closure body"))?;
    let parameters: Vec<ParameterSpec> = body
        .parameters
        .iter()
        .map(|parameter| ParameterSpec::user(&unit, parameter))
        .collect();
    Ok(Callee {
        argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
        name: body.name.as_str().into(),
        parameters: parameters.into(),
        target: CallTarget::User {
            unit,
            index: function.index(),
        },
    })
}
