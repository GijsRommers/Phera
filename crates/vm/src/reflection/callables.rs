//! `ReflectionMethod::invoke()` invokes the selected declaration, including private methods.

use std::rc::Rc;

use phera_extension_api::reflection::{FunctionTarget, MemberInfo};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Object, PhpArray, Value};

use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee};
use crate::classes::{Class, Method, MethodBody};
use crate::closures::{ClosureOrigin, ClosureState};
use crate::code_table::FunctionIndex;

use super::exception;

pub(super) fn closure_invoke_receiver(
    receiver: Option<Rc<Object>>,
) -> Result<Rc<Object>, NativeError> {
    let object = receiver.ok_or_else(|| {
        exception("Trying to invoke non static method Closure::__invoke() without an object")
    })?;
    if !object.class.is_instance_of("Closure") {
        return Err(exception(
            "Given object is not an instance of the class this method was declared in",
        ));
    }
    Ok(object)
}

impl Vm {
    pub(super) fn reflection_function_callee(
        &mut self,
        target: &FunctionTarget,
        receiver: Option<Rc<Object>>,
    ) -> Result<Callee, NativeError> {
        let mut callee = match target {
            FunctionTarget::Named(name) => self.resolve_callee(name)?,
            FunctionTarget::Closure(object) => {
                let object = object
                    .upgrade()
                    .ok_or_else(|| exception("Closure is unavailable"))?;
                self.resolve_callable(&Value::Object(object))?
            }
            FunctionTarget::ClosureInvoke(_) => {
                let object = closure_invoke_receiver(receiver)?;
                self.resolve_callable(&Value::Object(object))?
            }
            FunctionTarget::Method { class, name } => {
                let (class, method) = self.reflection_method(class, name)?;
                let scope = self
                    .classes
                    .get(&method.declaring)
                    .ok_or_else(|| exception("Method scope does not exist"))?;
                if method.is_abstract {
                    return Err(exception(format!(
                        "Trying to invoke abstract method {}::{}()",
                        method.declaring, method.name
                    )));
                }
                let called = if method.is_static {
                    class
                } else {
                    let object = receiver.as_ref().ok_or_else(|| {
                        exception(format!(
                            "Trying to invoke non static method {}::{}() without an object",
                            method.declaring, method.name,
                        ))
                    })?;
                    self.classes.for_object(object)
                        .filter(|actual| actual.is_a(&scope.name))
                        .ok_or_else(|| exception("Given object is not an instance of the class this method was declared in"))?
                };
                self.reflection_callee(called, scope, method, receiver)
            }
        };
        callee.argument_binding = crate::calls::callback_binding::ArgumentBinding::Internal;
        Ok(callee)
    }

    pub(super) fn reflection_method(
        &self,
        class: &str,
        name: &str,
    ) -> Result<(Rc<Class>, Rc<Method>), NativeError> {
        let class = self
            .classes
            .get(class)
            .ok_or_else(|| exception(format!("Class \"{class}\" does not exist")))?;
        let method = class
            .methods
            .get(&name.to_ascii_lowercase())
            .cloned()
            .ok_or_else(|| exception(format!("Method {}::{name}() does not exist", class.name)))?;
        Ok((class, method))
    }

    pub(super) fn reflection_prototype(&self, member: &MemberInfo) -> Option<MemberInfo> {
        let class = self.classes.get(&member.lookup_class)?;
        let owner = self.classes.get(&member.class)?;
        let interfaces = crate::class_listings::interfaces_in_php_order(&class);
        for candidate in interfaces {
            if let Some(prototype) = self.reflection_methods(candidate).into_iter().find(|item| {
                item.name.eq_ignore_ascii_case(&member.name) && item.class != member.class
            }) {
                return Some(prototype);
            }
        }
        let mut next = owner.parent.as_deref();
        while let Some(parent) = next {
            if let Some(prototype) = self.reflection_methods(parent).into_iter().find(|item| {
                item.name.eq_ignore_ascii_case(&member.name)
                    && item.modifiers & 4 == 0
                    && (!member.name.eq_ignore_ascii_case("__construct")
                        || item.modifiers & 64 != 0)
            }) {
                return Some(prototype);
            }
            next = parent.parent.as_deref();
        }
        None
    }

    pub(super) fn reflection_statics(&mut self, target: &FunctionTarget) -> NativeResult {
        let mut values = PhpArray::new();
        let function = match target {
            FunctionTarget::Named(name) => function_of(&self.resolve_callee(name)?.target),
            FunctionTarget::Method { class, name } => {
                method_function(&self.reflection_method(class, name)?.1)
            }
            FunctionTarget::ClosureInvoke(_) => None,
            FunctionTarget::Closure(object) => {
                let object = object
                    .upgrade()
                    .ok_or_else(|| exception("Closure is unavailable"))?;
                let stored = object
                    .native
                    .try_borrow()
                    .map_err(|_| exception("Closure is in use"))?;
                let closure = stored
                    .as_ref()
                    .and_then(|state| state.downcast_ref::<ClosureState>())
                    .ok_or_else(|| exception("Closure is uninitialized"))?;
                if let Some(function) = function_of(&closure.callee.target)
                    && closure.origin == ClosureOrigin::Lexical
                {
                    let unit = self.code.unit(function);
                    let body = self.code.function(function);
                    values = self.reflection_captures(function, closure);
                    let statics = closure.statics.as_ref().map(|statics| statics.borrow());
                    for (index, name) in body.statics.iter().enumerate() {
                        let value = statics
                            .as_ref()
                            .and_then(|statics| statics.get(&(index as u16)))
                            .map_or(Value::Null, |value| value.deref().clone());
                        values.insert(ArrayKey::str(unit.names[name.index()].as_str()), value);
                    }
                    return Ok(Value::from(values));
                }
                function_of(&closure.callee.target)
            }
        };
        if let Some(function) = function {
            let unit = self.code.unit(function);
            let body = self.code.function(function);
            for (index, name) in body.statics.iter().enumerate() {
                let key = crate::references::StaticKey {
                    unit: std::sync::Arc::as_ptr(unit) as usize,
                    function: self.code.local(function),
                    variable: index as u16,
                };
                let value = self
                    .static_variables
                    .get(&key)
                    .map_or(Value::Null, |value| value.deref().clone());
                values.insert(ArrayKey::str(unit.names[name.index()].as_str()), value);
            }
        }
        Ok(Value::from(values))
    }

    pub(super) fn reflection_closure_uses(&self, target: &FunctionTarget) -> NativeResult {
        let FunctionTarget::Closure(object) = target else {
            return Ok(Value::from(PhpArray::new()));
        };
        let object = object
            .upgrade()
            .ok_or_else(|| exception("Closure is unavailable"))?;
        let storage = object
            .native
            .try_borrow()
            .map_err(|_| exception("Closure is in use"))?;
        let closure = storage
            .as_ref()
            .and_then(|state| state.downcast_ref::<ClosureState>())
            .ok_or_else(|| exception("Closure is uninitialized"))?;
        Ok(Value::from(
            function_of(&closure.callee.target).map_or_else(PhpArray::new, |function| {
                self.reflection_captures(function, closure)
            }),
        ))
    }

    pub(super) fn reflection_captures(
        &self,
        function: FunctionIndex,
        closure: &ClosureState,
    ) -> PhpArray {
        let unit = self.code.unit(function);
        let body = self.code.function(function);
        let mut values = PhpArray::new();
        for (name, value) in body
            .locals
            .iter()
            .skip(body.parameters.len())
            .zip(&closure.captures)
        {
            let key = ArrayKey::str(unit.names[name.index()].as_str());
            if let Some(value) = value {
                values.insert(key, value.deref().clone());
            }
        }
        values
    }
}

pub(super) fn function_of(target: &CallTarget) -> Option<FunctionIndex> {
    match target {
        CallTarget::User(function) => Some(*function),
        CallTarget::Method { method, .. } => method_function(method),
        _ => None,
    }
}

fn method_function(method: &Method) -> Option<FunctionIndex> {
    match method.body {
        MethodBody::User(function) => Some(function),
        _ => None,
    }
}
