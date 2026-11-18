//! `spl_autoload_register($loader, prepend: true)` inserts a callable at the front of this request's list.

use std::rc::Rc;

use phera_extension_api::{NativeError, NativeResult, Param, ParamType};
use phera_value::{PhpArray, Value};

use crate::{
    Vm,
    autoload::RegisteredLoader,
    calls::{
        call_state::{CallTarget, Callee},
        callback_binding::ArgumentOrigin,
    },
    intrinsics::Intrinsic,
};

#[derive(Clone, Copy)]
pub(crate) enum SplAutoload {
    Register,
    Unregister,
    Functions,
    Call,
    Extensions,
    DefaultLoader,
}

impl SplAutoload {
    pub(crate) const ALL: &[Self] = &[
        Self::Register,
        Self::Unregister,
        Self::Functions,
        Self::Call,
        Self::Extensions,
        Self::DefaultLoader,
    ];

    pub(crate) fn resolve(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|function| function.name() == name)
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Register => "spl_autoload_register",
            Self::Unregister => "spl_autoload_unregister",
            Self::Functions => "spl_autoload_functions",
            Self::Call => "spl_autoload_call",
            Self::Extensions => "spl_autoload_extensions",
            Self::DefaultLoader => "spl_autoload",
        }
    }

    pub(crate) fn parameters(self) -> &'static [Param] {
        const CLASS: Param = Param::new("class", ParamType::String);
        const EXTENSIONS: Param = Param::new("file_extensions", ParamType::String)
            .optional()
            .nullable();
        match self {
            Self::Register => {
                const {
                    &[
                        Param::new("callback", ParamType::Mixed).optional(),
                        Param::new("throw", ParamType::Bool).optional(),
                        Param::new("prepend", ParamType::Bool).optional(),
                    ]
                }
            }
            Self::Unregister => const { &[Param::new("callback", ParamType::Mixed)] },
            Self::Functions => &[],
            Self::Call => const { &[CLASS] },
            Self::Extensions => const { &[EXTENSIONS] },
            Self::DefaultLoader => const { &[CLASS, EXTENSIONS] },
        }
    }
}

impl Vm {
    pub(crate) async fn spl_autoload_intrinsic(
        &mut self,
        function: SplAutoload,
        mut arguments: Vec<Value>,
        origin: ArgumentOrigin,
    ) -> NativeResult {
        use SplAutoload as Operation;
        self.with_argument_origin(origin, |vm| {
            phera_extension_api::prepare_call(
                function.name(),
                function.parameters(),
                &mut arguments,
                vm,
            )
        })?;
        match function {
            Operation::Register | Operation::Unregister => {
                self.change_autoloader(function, &arguments).await
            }
            Operation::Functions => {
                let mut listed = PhpArray::new();
                for loader in self.autoload.callbacks.iter().flatten() {
                    listed.push(loader.display.clone())?;
                }
                Ok(Value::from(listed))
            }
            Operation::Call => {
                self.call_autoloaders(&string(&arguments[0])).await?;
                Ok(Value::Null)
            }
            Operation::Extensions => {
                if let Some(value) = arguments.first().filter(|value| !value.is_null()) {
                    self.autoload.extensions = Some(string(value));
                }
                Ok(Value::from(self.autoload_extensions()))
            }
            Operation::DefaultLoader => {
                let extensions = arguments
                    .get(1)
                    .filter(|value| !value.is_null())
                    .map(string);
                self.default_autoload(&string(&arguments[0]), extensions)
                    .await?;
                Ok(Value::Null)
            }
        }
    }

    async fn change_autoloader(
        &mut self,
        operation: SplAutoload,
        arguments: &[Value],
    ) -> NativeResult {
        let register = matches!(operation, SplAutoload::Register);
        let callback = arguments.first().cloned().unwrap_or_default();
        let default = register && callback.is_null();
        let callee = if default {
            self.resolve_callee("spl_autoload")?
        } else {
            self.autoload_callable(&callback).await?;
            self.resolve_callable(&callback).map_err(|error| {
                self.invalid_callback(operation.name(), 1, "callback", &callback, error, register)
            })?
        };
        if register
            && arguments
                .get(1)
                .is_some_and(|value| !phera_value::to_bool(value))
        {
            self.diagnostic("Notice", "spl_autoload_register(): Argument #2 ($do_throw) has been ignored, spl_autoload_register() will always throw".into());
        }
        if matches!(
            callee.target,
            CallTarget::Intrinsic(Intrinsic::SplAutoload(SplAutoload::Call))
        ) {
            return Err(if register {
                NativeError::value_error(
                    "spl_autoload_register(): Argument #1 ($callback) must not be the spl_autoload_call() function",
                )
            } else {
                crate::function_introspection::deprecated(
                    "spl_autoload_unregister('spl_autoload_call')",
                    "8.5",
                )
            });
        }
        let found = self.autoload.callbacks.iter().position(|slot| {
            slot.as_ref()
                .is_some_and(|loader| same_loader(&loader.callee, &callee))
        });
        if !register {
            if let Some(position) = found {
                self.autoload.callbacks[position] = None;
            }
            return Ok(Value::Bool(found.is_some()));
        }
        if found.is_none() {
            let display = display_callable(&callee, &callback)?;
            let loader = Some(RegisteredLoader { callee, display });
            if arguments.get(2).is_some_and(phera_value::to_bool) {
                self.autoload.callbacks.insert(0, loader);
            } else {
                self.autoload.callbacks.push(loader);
            }
        }
        Ok(Value::Bool(true))
    }

    pub(crate) fn autoload_extensions(&self) -> &str {
        self.autoload.extensions.as_deref().unwrap_or(".inc,.php")
    }
}

fn same_loader(left: &Callee, right: &Callee) -> bool {
    match (&left.target, &right.target) {
        (CallTarget::Closure(left), CallTarget::Closure(right)) => Rc::ptr_eq(left, right),
        (
            CallTarget::Method {
                method: left,
                context: lc,
            },
            CallTarget::Method {
                method: right,
                context: rc,
            },
        ) => {
            Rc::ptr_eq(left, right)
                && Rc::ptr_eq(&lc.called, &rc.called)
                && match (&lc.receiver, &rc.receiver) {
                    (None, None) => true,
                    (Some(left), Some(right)) => Rc::ptr_eq(left, right),
                    _ => false,
                }
        }
        (CallTarget::User(left), CallTarget::User(right)) => left == right,
        (CallTarget::Native(left), CallTarget::Native(right)) => left.name == right.name,
        (CallTarget::Intrinsic(_), CallTarget::Intrinsic(_)) => left.name == right.name,
        _ => false,
    }
}

fn display_callable(callee: &Callee, original: &Value) -> NativeResult {
    if matches!(original, Value::Object(object) if object.class.name() == "Closure") {
        return Ok(original.clone());
    }
    if let CallTarget::Method { method, context } = &callee.target {
        let mut pair = PhpArray::new();
        pair.push(
            context
                .receiver
                .clone()
                .map(Value::Object)
                .unwrap_or_else(|| context.called.name_value.clone()),
        )?;
        pair.push(Value::from(method.name.as_str()))?;
        return Ok(Value::from(pair));
    }
    Ok(Value::from(callee.name.as_ref()))
}

fn string(value: &Value) -> String {
    match value {
        Value::String(value) => value.to_string_lossy(),
        _ => String::new(),
    }
}
