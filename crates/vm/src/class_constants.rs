//! Class constants evaluate once in their declaring scope and reject dependency cycles.

use crate::{Vm, classes::ConstantState};
use phera_bytecode::{ClassKind, Reg, Visibility};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;

impl Vm {
    pub(crate) fn dynamic_class_constant(&mut self, class: &str, name: &Value) -> NativeResult {
        let Value::String(name) = name else {
            return Err(NativeError::type_error(format!(
                "Cannot use value of type {} as class constant name",
                name.type_name()
            )));
        };
        self.class_constant(class, &name.to_string_lossy())
    }
    /// Both objects and strings are valid dynamic class targets.
    pub(crate) fn class_target(&mut self, register: Reg) -> Result<String, NativeError> {
        match self.read_local(register) {
            Value::Object(object) => Ok(object.class.name().to_owned()),
            Value::String(name) => Ok(name.to_string_lossy()),
            _ => Err(NativeError::error(
                "Class name must be a valid object or a string",
            )),
        }
    }

    pub(crate) fn fetch_dynamic_class_constant(
        &mut self,
        destination: Reg,
        class: Reg,
        name: Reg,
    ) -> Result<(), NativeError> {
        let class = self.class_target(class)?;
        let name = self.read_local(name);
        let value = self.dynamic_class_constant(&class, &name)?;
        self.set(destination, value);
        Ok(())
    }

    /// Resolve inherited constants while retaining the scope of their original declaration.
    pub(crate) fn class_constant(&mut self, class: &str, name: &str) -> NativeResult {
        self.class_constant_written(class, name, None)
    }

    pub(crate) fn class_constant_written(
        &mut self,
        class: &str,
        name: &str,
        spelling: Option<&str>,
    ) -> NativeResult {
        let linked = self.classes.get(class);
        let (display, constants) = if let Some(linked) = &linked {
            (&linked.name, &linked.constants)
        } else if let Some(pending) = self
            .initializer_class
            .as_ref()
            .filter(|pending| pending.name.eq_ignore_ascii_case(class))
        {
            (&pending.name, &pending.constants)
        } else {
            return Err(Self::class_miss(class, "Class"));
        };
        let constant = constants
            .get(name)
            .cloned()
            .ok_or_else(|| NativeError::error(format!("Undefined constant {display}::{name}")))?;
        if let Some(linked) = &linked {
            if linked.kind == ClassKind::Trait {
                return Err(NativeError::error(format!(
                    "Cannot access trait constant {display}::{name} directly"
                )));
            }
            if linked.kind == ClassKind::Enum {
                self.validate_enum_backing(linked)?;
            }
        }
        if !self.can_read_class_constant(&constant) {
            let visibility = if constant.visibility == Visibility::Private {
                "private"
            } else {
                "protected"
            };
            return Err(NativeError::error(format!(
                "Cannot access {visibility} constant {}::{name}",
                constant.declaring
            )));
        }
        if let Some(message) = constant.deprecated {
            self.diagnostic("Deprecated", message.into());
        }
        if let crate::classes::class::ConstantInitializer::EnumCase { name, backing } =
            &constant.initializer
        {
            let class = linked
                .as_ref()
                .ok_or_else(|| internal_error!("unlinked enum"))?;
            return self
                .enum_case(class, name, *backing, spelling)
                .map(Value::Object);
        }
        self.evaluate_class_constant(&constant, name)
    }

    pub(crate) fn can_read_class_constant(
        &self,
        constant: &crate::classes::LinkedConstant,
    ) -> bool {
        if self.initializer_scope.as_deref() == Some(constant.declaring.as_str()) {
            return true;
        }
        let scope = self
            .initializer_scope
            .as_ref()
            .and_then(|name| self.classes.get(name))
            .or_else(|| self.current().and_then(|frame| frame.scope().cloned()));
        match constant.visibility {
            Visibility::Public => true,
            Visibility::Private => scope.is_some_and(|scope| scope.name == constant.declaring),
            Visibility::Protected => scope.is_some_and(|scope| {
                scope.is_a(&constant.declaring)
                    || self
                        .classes
                        .get(&constant.declaring)
                        .is_some_and(|declaring| declaring.is_a(&scope.name))
            }),
        }
    }

    fn evaluate_class_constant(
        &mut self,
        constant: &crate::classes::LinkedConstant,
        name: &str,
    ) -> NativeResult {
        let (initializer, state) = match &constant.initializer {
            crate::classes::class::ConstantInitializer::EnumCase { .. } => {
                return Err(internal_error!(
                    "enum case reached ordinary constant evaluation"
                ));
            }
            crate::classes::class::ConstantInitializer::Value(value) => return Ok(value.clone()),
            crate::classes::class::ConstantInitializer::Compiled { function, state } => {
                (function, state)
            }
        };
        match &*state.borrow() {
            ConstantState::Ready(value) => return Ok(value.clone()),
            ConstantState::Evaluating => {
                return Err(NativeError::error(format!(
                    "Cannot declare self-referencing constant {}::{name}",
                    constant.declaring
                )));
            }
            ConstantState::Uninitialized => {}
        }
        *state.borrow_mut() = ConstantState::Evaluating;
        let result = self
            .evaluate_initializer(initializer, &constant.declaring)
            .and_then(|value| {
                if let Some(declaration_type) = &constant.ty {
                    let actual = value.type_name();
                    crate::types::coerce(
                        value,
                        declaration_type,
                        crate::types::TypeChecking::Strict,
                        self,
                    )
                    .map_err(|_| {
                        NativeError::type_error(format!(
                            "Cannot assign {actual} to class constant {}::{name} of type {}",
                            constant.declaring, declaration_type.display
                        ))
                    })
                } else {
                    Ok(value)
                }
            });
        *state.borrow_mut() = match &result {
            Ok(value) => ConstantState::Ready(value.clone()),
            Err(_) => ConstantState::Uninitialized,
        };
        result
    }
}
