//! Class constants evaluate once in their declaring scope and reject dependency cycles.

use crate::{
    Vm,
    classes::{ConstantState, LinkedConstant, class::ConstantInitializer},
    initializer_tasks::{CaseLookup, ConstantLookup, Continuation, InitializerTask, Progress},
};
use phera_bytecode::{ClassKind, Reg, Visibility};
use phera_extension_api::{NativeError, NativeResult};
use phera_source::deprecations::ItemKind;
use phera_value::Value;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstantAccess {
    Read,
    Defined,
    Reflect,
}

impl Vm {
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
        let linked = self.classes.get(&class).ok_or_else(|| {
            if let Value::String(name) = self.read_local(name) {
                Self::class_member_miss(&class, ItemKind::ClassConstant, &name.to_string_lossy())
            } else {
                Self::class_miss(&class, "Class")
            }
        })?;
        let name = constant_name(self.read_local(name))?;
        let value = if name.eq_ignore_ascii_case("class") {
            linked.name_value.clone()
        } else {
            self.class_constant(&class, &name)?
        };
        self.set(destination, value);
        Ok(())
    }

    /// Resolve inherited constants while retaining the scope of their original declaration.
    pub(crate) fn class_constant(&mut self, class: &str, name: &str) -> NativeResult {
        self.class_constant_with_access(class, name, ConstantAccess::Read)
    }

    pub(crate) fn class_constant_with_access(
        &mut self,
        class: &str,
        name: &str,
        access: ConstantAccess,
    ) -> NativeResult {
        let (linked, constant) = self.resolve_class_constant(class, name, access)?;
        // A linked value can return directly. The dependency stack is needed
        // only for unfinished initializers or the initial backing-table build.
        if !linked
            .as_ref()
            .is_some_and(|class| self.needs_enum_backing(class))
            && let Some(value) = ready_value(&constant.initializer)
        {
            self.check_constant_access(&constant, name, access)?;
            return Ok(value);
        }
        self.run_unresolved_constant(class, name, access)
    }

    #[cold]
    fn run_unresolved_constant(
        &mut self,
        class: &str,
        name: &str,
        access: ConstantAccess,
    ) -> NativeResult {
        self.run_initializer(InitializerTask::Constant(ConstantLookup {
            class: class.to_owned(),
            name: name.to_owned(),
            spelling: None,
            access,
        }))
    }

    pub(crate) fn start_class_constant(
        &mut self,
        lookup: ConstantLookup,
    ) -> Result<Progress, NativeError> {
        let ConstantLookup {
            class,
            name,
            spelling,
            access,
        } = &lookup;
        let (linked, constant) = self.resolve_class_constant(class, name, *access)?;
        if let Some(linked) = &linked
            && self.needs_enum_backing(linked)
        {
            return Ok(Progress::Await(
                InitializerTask::ValidateEnum(linked.clone()),
                Continuation::AfterValidation(lookup, self.initializer_scope.clone()),
            ));
        }
        self.check_constant_access(&constant, name, *access)?;
        match &constant.initializer {
            ConstantInitializer::EnumCase { name, backing } => {
                let class = linked.ok_or_else(|| internal_error!("unlinked enum"))?;
                Ok(Progress::Next(InitializerTask::Case(CaseLookup {
                    class,
                    name: name.clone(),
                    initializer: *backing,
                    spelling: spelling.clone(),
                })))
            }
            ConstantInitializer::Value(value) => Ok(Progress::Done(value.clone())),
            ConstantInitializer::Compiled { function, state } => {
                match &*state.borrow() {
                    ConstantState::Ready(value) => return Ok(Progress::Done(value.clone())),
                    ConstantState::Evaluating => {
                        let receiver = spelling.as_deref().unwrap_or(&constant.declaring);
                        return Err(NativeError::error(format!(
                            "Cannot declare self-referencing constant {receiver}::{name}"
                        )));
                    }
                    ConstantState::Uninitialized => {}
                }
                *state.borrow_mut() = ConstantState::Evaluating;
                let initializer =
                    InitializerTask::Expression(*function, constant.declaring.clone());
                Ok(Progress::Await(
                    initializer,
                    Continuation::Constant(constant, name.to_owned()),
                ))
            }
        }
    }

    fn resolve_class_constant(
        &self,
        class: &str,
        name: &str,
        access: ConstantAccess,
    ) -> Result<(Option<Rc<crate::classes::Class>>, Rc<LinkedConstant>), NativeError> {
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
            return Err(Self::class_member_miss(
                class,
                ItemKind::ClassConstant,
                name,
            ));
        };
        let constant = constants.get(name).cloned().ok_or_else(|| {
            linked
                .as_ref()
                .and_then(|class| {
                    crate::unsupported_items::omitted_native_member(
                        ItemKind::ClassConstant,
                        class,
                        name,
                    )
                })
                .unwrap_or_else(|| {
                    NativeError::error(format!("Undefined constant {display}::{name}"))
                })
        })?;
        if let Some(linked) = &linked
            && linked.kind == ClassKind::Trait
            && access != ConstantAccess::Reflect
        {
            return Err(NativeError::error(format!(
                "Cannot access trait constant {display}::{name} directly"
            )));
        }
        Ok((linked, constant))
    }

    fn check_constant_access(
        &mut self,
        constant: &LinkedConstant,
        name: &str,
        access: ConstantAccess,
    ) -> Result<(), NativeError> {
        if access != ConstantAccess::Reflect && !self.can_read_class_constant(constant) {
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
        if access == ConstantAccess::Read
            && let Some(message) = &constant.deprecated
        {
            self.diagnostic("Deprecated", message.clone());
        }
        Ok(())
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

    pub(crate) fn finish_class_constant(
        &mut self,
        constant: &LinkedConstant,
        name: &str,
        value: Value,
    ) -> NativeResult {
        let ConstantInitializer::Compiled { state, .. } = &constant.initializer else {
            return Err(internal_error!("class constant completion"));
        };
        let result = if let Some(declaration_type) = &constant.ty {
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
                    constant.declaring,
                    crate::types::php_type_display(declaration_type),
                ))
            })
        } else {
            Ok(value)
        };
        *state.borrow_mut() = match &result {
            Ok(value) => ConstantState::Ready(value.clone()),
            Err(_) => ConstantState::Uninitialized,
        };
        result
    }

    pub(crate) fn cancel_class_constant(constant: &LinkedConstant) {
        if let ConstantInitializer::Compiled { state, .. } = &constant.initializer {
            *state.borrow_mut() = ConstantState::Uninitialized;
        }
    }
}

fn ready_value(initializer: &ConstantInitializer) -> Option<Value> {
    match initializer {
        ConstantInitializer::Value(value) => Some(value.clone()),
        ConstantInitializer::Compiled { state, .. } => match &*state.borrow() {
            ConstantState::Ready(value) => Some(value.clone()),
            _ => None,
        },
        ConstantInitializer::EnumCase { .. } => None,
    }
}

pub(crate) fn constant_name(name: Value) -> Result<String, NativeError> {
    match name {
        Value::String(name) => Ok(name.to_string_lossy()),
        value => Err(NativeError::type_error(format!(
            "Cannot use value of type {} as class constant name",
            value.type_name(),
        ))),
    }
}
