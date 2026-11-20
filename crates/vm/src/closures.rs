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
use crate::code_table::FunctionIndex;

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
    pub(crate) statics: Option<Rc<RefCell<HashMap<u16, Value>>>>,
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
                let slot = self
                    .current_slot_mut(source)
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
            .base();
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
        let main = self.code.facts(frame.frame.function).unit_first;
        let (index, _) = self
            .code
            .declared_facts(main, function.index())
            .ok_or_else(|| internal_error!("closure body"))?;
        let body = self.code.function(index);
        let capture_count = body.captures as usize;
        let statics = (!body.statics.is_empty()).then(|| Rc::new(RefCell::new(HashMap::new())));
        let context = self.closure_context(binding);
        let callee = self.cached_closure_callee(frame.cache(), function, index);
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
            statics,
        };
        let object = self.allocate_closure(state)?;
        self.set(destination, Value::Object(object));
        Ok(())
    }

    /// Every evaluation of one closure expression shares immutable argument metadata.
    fn cached_closure_callee(
        &self,
        cache: &crate::unit_caches::UnitCache,
        local: FunctionId,
        function: FunctionIndex,
    ) -> Callee {
        let mut signatures = cache.closure_signatures.borrow_mut();
        let index = local.index();
        if signatures.len() <= index {
            signatures.resize_with(index + 1, || None);
        }
        signatures[index]
            .get_or_insert_with(|| closure_callee(&self.code, function))
            .clone()
    }

    /// A static closure keeps private class access while deliberately omitting the receiver.
    fn closure_context(&self, binding: ClosureBinding) -> Option<MethodContext> {
        let mut context = self.current()?.method()?.clone();
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
            .closure_class()
            .ok_or_else(|| internal_error!("Closure class"))?;
        let object = self.classes.allocate(class);
        *object.native.borrow_mut() = Some(Box::new(state));
        Ok(object)
    }
}

/// The variables a closure object captured, for describing where a value
/// was found (see `crate::snapshot`); empty for any other object.
pub(crate) fn captured_values(object: &phera_value::Object) -> Vec<Value> {
    let Ok(native) = object.native_payload() else {
        return Vec::new();
    };
    native
        .as_ref()
        .and_then(|state| state.downcast_ref::<ClosureState>())
        .map(|state| state.captures.iter().flatten().cloned().collect())
        .unwrap_or_default()
}

impl ClosureState {
    /// PHP clone and bind copy the current static values into independent reference cells.
    pub(crate) fn duplicate(&self) -> Self {
        let statics = self.statics.as_ref().map(|statics| {
            let copied = statics
                .borrow()
                .iter()
                .map(|(key, value)| {
                    let cloned = match value {
                        Value::Reference(reference) if Rc::strong_count(reference) > 1 => {
                            value.clone()
                        }
                        _ => Value::Reference(Rc::new(RefCell::new(value.deref().clone()))),
                    };
                    (*key, cloned)
                })
                .collect();
            Rc::new(RefCell::new(copied))
        });
        Self {
            origin: self.origin,
            callee: self.callee.clone(),
            captures: self.captures.clone(),
            context: self.context.clone(),
            binding: self.binding,
            statics,
        }
    }
}

impl Vm {
    /// Function-derived closures share the original named function's statics.
    pub(crate) fn lexical_closure(&self) -> Option<Rc<Object>> {
        let object = self.current()?.closure()?;
        let lexical = object
            .native
            .borrow()
            .as_ref()
            .and_then(|state| state.downcast_ref::<ClosureState>())
            .is_some_and(|state| state.origin == ClosureOrigin::Lexical);
        lexical.then(|| object.clone())
    }
}

/// The callee of a closure's body; captured values stay in each closure object.
fn closure_callee(code: &crate::code_table::CodeTable, function: FunctionIndex) -> Callee {
    let unit = code.unit(function);
    let body = code.function(function);
    let parameters: Vec<ParameterSpec> = body
        .parameters
        .iter()
        .map(|parameter| ParameterSpec::user(unit, parameter))
        .collect();
    Callee {
        argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
        name: body.name.as_str().into(),
        parameters: parameters.into(),
        target: CallTarget::User(function),
    }
}
