//! Static-style calls distinguish method lookup from PHP's forwarded called class.

use phera_bytecode::{ClassBinding, Reg, Target};
use phera_extension_api::NativeError;
use phera_value::Value;

use super::{Invocation, magic_calls::MethodSyntax};
use crate::Vm;

impl Vm {
    /// PHP permits computed method names only when their value is a string.
    pub(crate) fn method_name(&mut self, register: Reg) -> Result<String, NativeError> {
        let value = self.read_local(register);
        let Some(name) = value.str_ref() else {
            return Err(NativeError::error("Method name must be a string"));
        };
        Ok(name.to_string_lossy())
    }

    /// Dynamic object calls use the same visibility and lexical private lookup as literals.
    pub(crate) fn dynamic_method_call(
        &mut self,
        receiver: Reg,
        method: Reg,
    ) -> Result<(), NativeError> {
        let name = self.method_name(method)?;
        let value = self.read_local(receiver);
        let Value::Object(object) = value else {
            return Err(NativeError::error(format!(
                "Call to a member function {name}() on {}",
                value.type_name()
            )));
        };
        if crate::generators::generator_object(&object) {
            self.discard_generator_temporary(receiver);
        }
        let class = self
            .classes
            .for_object(&object)
            .ok_or_else(|| internal_error!("receiver class"))?;
        self.prepare_method(class, Some(object), &name, Invocation::Method)
    }

    /// A compatible current receiver survives static syntax; keywords also forward static.
    pub(crate) fn static_style_call(
        &mut self,
        class: Reg,
        method: Reg,
        binding: ClassBinding,
    ) -> Result<(), NativeError> {
        let class_value = self.read_local(class);
        let class_name: std::borrow::Cow<'_, str> = match &class_value {
            Value::Object(object) => std::borrow::Cow::Borrowed(object.class.name()),
            Value::String(name) => String::from_utf8_lossy(name.as_bytes()),
            _ => {
                return Err(NativeError::error(
                    "Class name must be a valid object or a string",
                ));
            }
        };
        let method_value = self.read_local(method);
        let Value::String(method_ref) = &method_value else {
            return Err(NativeError::error("Method name must be a string"));
        };
        let method_name = String::from_utf8_lossy(method_ref.as_bytes());
        self.prepare_static_style(&class_name, &method_name, binding)
    }

    /// Resolve lookup scope separately from the late-bound scope saved on the pending call.
    pub(crate) fn prepare_static_style(
        &mut self,
        name: &str,
        method: &str,
        binding: ClassBinding,
    ) -> Result<(), NativeError> {
        let class = self.classes.get(name).ok_or_else(|| {
            Self::class_member_miss(name, phera_source::deprecations::ItemKind::Method, method)
        })?;
        let context = self.current().and_then(|frame| frame.method());
        let receiver = context
            .as_ref()
            .and_then(|context| context.receiver.clone())
            .filter(|object| {
                self.classes
                    .for_object(object)
                    .is_some_and(|receiver_class| self.class_matches(&receiver_class, name))
            });
        let resolved = self.find_method(&class, method, Invocation::Method);
        self.prepare_method_lookup(class, receiver, method, MethodSyntax::Static, resolved)?;
        if binding == ClassBinding::Forward
            && let Some(context) = context
        {
            self.forward_callback_class(context.called.clone())?;
        }
        Ok(())
    }

    /// Unknown instanceof targets are false; scalar targets other than strings are errors.
    pub(crate) fn dynamic_instance_of(
        &mut self,
        destination: Reg,
        source: Reg,
        class: Reg,
    ) -> Result<(), NativeError> {
        let name = self
            .class_target(class)
            .map_err(|_| NativeError::error("Class name must be a valid object or a string"))?;
        let matches = match self.read_local(source) {
            Value::Object(object) => self
                .classes
                .for_object(&object)
                .is_some_and(|class| self.class_matches(&class, &name)),
            _ => false,
        };
        self.set(destination, Value::Bool(matches));
        Ok(())
    }

    /// The evaluated-target form of `Vm::instance_of_branch`.
    pub(crate) fn dynamic_instance_of_branch(
        &mut self,
        source: Reg,
        class: Reg,
        target: Target,
        when: bool,
        work: &mut usize,
    ) -> Result<(), NativeError> {
        let name = self
            .class_target(class)
            .map_err(|_| NativeError::error("Class name must be a valid object or a string"))?;
        let matches = match self.read_local(source) {
            Value::Object(object) => self
                .classes
                .for_object(&object)
                .is_some_and(|class| self.class_matches(&class, &name)),
            _ => false,
        };
        if matches == when {
            if self.backward(target) {
                *work += 1;
            }
            self.jump(target)?;
        }
        Ok(())
    }
}
