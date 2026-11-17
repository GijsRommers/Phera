//! Constant callable expressions create objects without invoking them: `public $f = strlen(...);`.

use std::rc::Rc;

use phera_bytecode::{Instruction, Reg};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;
use crate::call_state::{CallTarget, Callee, ParameterSpec};
use crate::classes::{Invocation, MethodContext};
use crate::closures::{ClosureOrigin, ClosureState};
use crate::function_table::FunctionRef;

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
        reference: &FunctionRef,
        scope: &str,
        prepared: &mut Option<Rc<Callee>>,
        slots: &[Option<Value>],
    ) -> Result<InitializerCallable, NativeError> {
        match instruction {
            Instruction::InitCall { function } => {
                let symbol = &reference.unit.symbols[function.index()];
                *prepared = Some(self.resolve_callee(symbol.written.clone())?);
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
                let name = &reference.unit.symbols[class.index()].written;
                let class = self.initializer_class(name, scope)?;
                let method = &reference.unit.symbols[method.index()].written;
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
        reference: &FunctionRef,
        function: phera_bytecode::FunctionId,
        scope: &str,
        binding: phera_bytecode::ClosureBinding,
    ) -> Result<Rc<phera_value::Object>, NativeError> {
        let index = function.index();
        let body = &reference.unit.functions[index];
        let parameters = body
            .parameters
            .iter()
            .map(|parameter| ParameterSpec {
                name: reference.unit.names[parameter.name.index()].clone(),
                by_reference: parameter.by_reference,
                variadic: parameter.variadic,
            })
            .collect();
        let context = self.classes.get(scope).map(|scope| MethodContext {
            called: scope.clone(),
            scope,
            receiver: None,
            result_plan: None,
            invocation: Invocation::Method,
        });
        let callee = Rc::new(Callee {
            argument_binding: crate::callback_binding::ArgumentBinding::Caller,
            name: body.name.as_str().into(),
            parameters,
            target: CallTarget::User(FunctionRef {
                unit: reference.unit.clone(),
                index,
            }),
        });
        let state = ClosureState {
            origin: ClosureOrigin::Lexical,
            callee,
            context,
            binding,
            captures: Vec::new(),
            statics: Default::default(),
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
