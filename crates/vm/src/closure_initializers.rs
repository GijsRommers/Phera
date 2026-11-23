//! Constant callable expressions create objects without invoking them: `public $f = strlen(...);`.

use std::rc::Rc;

use phera_bytecode::{Instruction, Reg};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee, ParameterSpec};
use crate::classes::{Invocation, MethodContext};
use crate::closures::{ClosureOrigin, ClosureState};
use crate::code_table::FunctionIndex;

/// Initializers prepare a target, produce a value, or continue scalar evaluation.
pub(crate) enum InitializerCallable {
    Unrelated,
    Prepared,
    Value(Reg, Value),
}

impl Vm {
    /// Constant expressions already contain resolved names; no PHP body runs during preparation.
    pub(crate) fn initializer_callable(
        &mut self,
        instruction: Instruction,
        reference: &FunctionIndex,
        scope: &str,
        prepared: &mut Option<Callee>,
        slots: &[Option<Value>],
    ) -> Result<InitializerCallable, NativeError> {
        match instruction {
            Instruction::InitCall { function } => {
                let symbol = self.code.unit(*reference).symbols[function.index()]
                    .written
                    .clone();
                *prepared = Some(self.resolve_callee(&symbol)?);
                Ok(InitializerCallable::Prepared)
            }
            Instruction::InitDynamicCall { callee } => {
                let value = slots
                    .get(callee.index())
                    .and_then(Option::as_ref)
                    .ok_or_else(|| internal_error!("initializer callable value"))?;
                *prepared = Some(
                    self.resolve_callable_for(value, crate::callables::CallableUse::FirstClass)?,
                );
                Ok(InitializerCallable::Prepared)
            }
            Instruction::InitStaticCall { class, method, .. } => {
                let unit = self.code.unit(*reference).clone();
                let name = &unit.symbols[class.index()].written;
                let class = self.initializer_class(name, scope)?;
                let method = &unit.symbols[method.index()].written;
                self.prepare_method(class, None, method, Invocation::Method)?;
                *prepared = Some(self.take_resolved_callable()?);
                Ok(InitializerCallable::Prepared)
            }
            Instruction::MakeCallable { dst } => {
                let callee = prepared
                    .take()
                    .ok_or_else(|| internal_error!("initializer callable"))?;
                let object = self.closure_from_callee(callee)?;
                Ok(InitializerCallable::Value(dst, Value::Object(object)))
            }
            Instruction::MakeClosure {
                dst,
                function,
                binding,
            } => {
                let object = self.initializer_closure(reference, function, scope, binding)?;
                Ok(InitializerCallable::Value(dst, Value::Object(object)))
            }
            _ => Ok(InitializerCallable::Unrelated),
        }
    }

    /// Static literal closures have lexical scope but no receiver or captured locals.
    fn initializer_closure(
        &mut self,
        reference: &FunctionIndex,
        function: phera_bytecode::FunctionId,
        scope: &str,
        binding: phera_bytecode::ClosureBinding,
    ) -> Result<Rc<phera_value::Object>, NativeError> {
        let main = self.code.facts(*reference).unit_first;
        let index = self.code.declared(main, function.index());
        let body = self.code.function(index);
        let parameters = body
            .parameters
            .iter()
            .map(|parameter| ParameterSpec::user(self.code.unit(index), parameter))
            .collect();
        let name = body.name.as_str().into();
        let context = self.classes.get(scope).map(|scope| MethodContext {
            called: scope.clone(),
            scope,
            receiver: None,
            invocation: Invocation::Method,
        });
        let callee = Callee {
            argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
            name,
            parameters,
            target: CallTarget::User(index),
        };
        let state = ClosureState {
            origin: ClosureOrigin::Lexical,
            callee,
            context,
            binding,
            captures: Vec::new(),
            statics: (!self.code.function(index).statics.is_empty())
                .then(|| Rc::new(std::cell::RefCell::new(Default::default()))),
        };
        let object = self.allocate_closure(state)?;
        Ok(object)
    }

    fn initializer_class(
        &self,
        name: &str,
        scope: &str,
    ) -> Result<Rc<crate::classes::Class>, NativeError> {
        let class = match name {
            "self" | "static" => self.classes.get(scope),
            "parent" => self
                .classes
                .get(scope)
                .and_then(|class| class.parent.clone()),
            _ => self.classes.get(name),
        };
        class.ok_or_else(|| NativeError::error(format!("Class \"{name}\" not found")))
    }
}
