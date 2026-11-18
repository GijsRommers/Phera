//! `new App\Service` asks registered SPL callbacks only after class lookup misses.

use std::{collections::HashSet, rc::Rc};

use phera_bytecode::Instruction;
use phera_extension_api::{NativeError, Unwind};
use phera_value::Value;

use crate::{Vm, calls::call_state::Callee, classes::Class};

#[derive(Default)]
pub(crate) struct Autoloaders {
    pub(crate) callbacks: Vec<Option<RegisteredLoader>>,
    pub(crate) extensions: Option<String>,
    active: HashSet<String>,
}

#[derive(Clone)]
pub(crate) struct RegisteredLoader {
    pub(crate) callee: Callee,
    pub(crate) display: Value,
}

/// Synchronous class-fetch instructions can suspend before they have a class to act on.
pub(crate) struct ClassMiss {
    name: String,
    kind: &'static str,
}

impl ClassMiss {
    pub(crate) fn take(error: NativeError) -> Result<Self, NativeError> {
        match error {
            NativeError::Unwind(Unwind(payload)) => payload
                .downcast::<Self>()
                .map(|miss| *miss)
                .map_err(|payload| NativeError::Unwind(Unwind(payload))),
            error => Err(error),
        }
    }

    pub(crate) fn is(error: &NativeError) -> bool {
        matches!(error, NativeError::Unwind(Unwind(payload)) if payload.is::<Self>())
    }

    fn error(&self) -> NativeError {
        NativeError::error(format!("{} \"{}\" not found", self.kind, self.name))
    }
}

impl Vm {
    /// This path runs only after the ordinary class table lookup has failed.
    #[cold]
    pub(crate) fn class_miss(&self, name: &str, kind: &'static str) -> NativeError {
        let miss = ClassMiss {
            name: name.into(),
            kind,
        };
        NativeError::Unwind(Unwind(Box::new(miss)))
    }

    /// Most lookups finish immediately. Each failed lookup gets one pass through the loaders.
    pub(crate) async fn lookup_class(
        &mut self,
        name: &str,
    ) -> Result<Option<Rc<Class>>, NativeError> {
        if let Some(class) = self.classes.get(name) {
            return Ok(Some(class));
        }
        let callback_name = name.strip_prefix('\\').unwrap_or(name);
        if self.autoload.callbacks.is_empty()
            || callback_name.is_empty()
            || !callback_name.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\\') || byte >= 128
            })
        {
            return Ok(None);
        }
        let key = callback_name.to_ascii_lowercase();
        if !self.autoload.active.insert(key.clone()) {
            return Ok(None);
        }
        let result = self.call_autoloaders(callback_name).await;
        self.autoload.active.remove(&key);
        result?;
        Ok(self.classes.get(name))
    }

    /// Read the live list after each call so removing or appending loaders takes effect.
    /// Explicit `spl_autoload_call()` also calls the first loader for an existing name.
    pub(crate) async fn call_autoloaders(&mut self, name: &str) -> Result<(), NativeError> {
        let mut position = 0;
        while let Some(slot) = self.autoload.callbacks.get(position) {
            let loader = slot.clone();
            position += 1;
            let Some(loader) = loader else { continue };
            self.invoke_resolved_callable(loader.callee, [Value::from(name)])
                .await?;
            if self
                .classes
                .entries
                .contains_key(&name.to_ascii_lowercase())
            {
                break;
            }
        }
        Ok(())
    }

    /// Resume the interrupted operation only after the class is linked. Operands were
    /// already evaluated into registers before the fetch instruction was entered.
    pub(crate) async fn resume_class_fetch(
        &mut self,
        instruction: Instruction,
        mut miss: ClassMiss,
        stop: usize,
        work: &mut usize,
    ) -> Result<Option<crate::calls::execution_loop::BatchExit>, NativeError> {
        loop {
            if !self.error_handlers.pending.is_empty() {
                self.drain_error_handlers().await?;
            }
            if self.lookup_class(&miss.name).await?.is_none() {
                return Err(miss.error());
            }
            match self.execute(instruction, stop, work) {
                Ok(exit) => return Ok(exit),
                Err(error) => miss = ClassMiss::take(error)?,
            }
        }
    }

    /// Named static callbacks can trigger the same class lookup before their method is resolved.
    pub(crate) async fn autoload_callable(&mut self, callable: &Value) -> Result<(), NativeError> {
        if self.autoload.callbacks.is_empty() {
            return Ok(());
        }
        let name = match &*callable.deref() {
            Value::String(text) => text
                .to_string_lossy()
                .split_once("::")
                .map(|(class, _)| class.to_owned()),
            Value::Array(values) if values.len() == 2 => {
                match (
                    values.get(&phera_value::ArrayKey::Int(0)),
                    values.get(&phera_value::ArrayKey::Int(1)),
                ) {
                    (Some(class), Some(method)) if matches!(&*method.deref(), Value::String(_)) => {
                        match &*class.deref() {
                            Value::String(name) => Some(name.to_string_lossy()),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(name) = name
            && !matches!(
                name.to_ascii_lowercase().as_str(),
                "self" | "static" | "parent"
            )
        {
            self.lookup_class(&name).await?;
        }
        Ok(())
    }
}

impl Vm {
    /// Native callback parameters use the loader before validating the method.
    pub(crate) async fn autoload_native_parameters(
        &mut self,
        parameters: &[phera_extension_api::Param],
        arguments: &[Value],
    ) -> Result<(), NativeError> {
        if self.autoload.callbacks.is_empty() {
            return Ok(());
        }
        for (parameter, argument) in parameters.iter().zip(arguments) {
            if parameter.ty == phera_extension_api::ParamType::Callable {
                self.autoload_callable(argument).await?;
            }
        }
        Ok(())
    }
}
