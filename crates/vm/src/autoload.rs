//! `new App\Service` asks registered SPL callbacks only after class lookup misses.

use std::{collections::HashSet, rc::Rc};

use phera_extension_api::{NativeError, Unwind};
use phera_value::Value;

use crate::{Vm, calls::call_state::Callee, classes::Class, error_traces::NativeTrace};

#[derive(Default)]
pub(crate) struct Autoloaders {
    pub(crate) callbacks: Vec<Option<RegisteredLoader>>,
    pub(crate) extensions: Option<String>,
    pub(crate) active: HashSet<String>,
    pub(crate) declaring: Vec<DeclaringClass>,
    pub(super) pending: Vec<PendingClass>,
}

#[derive(Clone)]
pub(crate) struct RegisteredLoader {
    pub(crate) callee: Callee,
    pub(crate) display: Value,
}

/// Synchronous class-fetch instructions can suspend before they have a class to act on.
///
/// The interpreter runs such an instruction again once the class is loaded
/// (`Vm::resume_class_fetch`). A native call cannot be run again that way,
/// because it has taken its prepared call and arguments by then: the native
/// call boundaries load the class themselves and call the native again
/// (`calls::native_retries`).
pub(crate) struct ClassMiss {
    pub(crate) name: String,
    failure: MissFailure,
    declaring: Option<DeclaringClass>,
}

mod method_variance;
pub(crate) use method_variance::{DeclaringClass, MethodVariance, PendingClass};

enum MissFailure {
    Lookup(&'static str),
    Member(phera_source::deprecations::ItemKind, String),
    MethodSignatures(PendingClass),
    PropertySignature {
        remaining: Vec<String>,
        failure: crate::error_traces::DeclarationFailure,
    },
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

    pub(crate) fn method_signatures(pending: PendingClass) -> NativeError {
        let name = pending.class.name.clone();
        NativeError::Unwind(Unwind(Box::new(Self {
            name,
            failure: MissFailure::MethodSignatures(pending),
            declaring: None,
        })))
    }

    pub(crate) fn during_declaration(
        mut error: NativeError,
        unit: &std::sync::Arc<phera_bytecode::CompiledUnit>,
        index: phera_bytecode::ClassId,
    ) -> NativeError {
        if let NativeError::Unwind(Unwind(payload)) = &mut error
            && let Some(miss) = payload.downcast_mut::<Self>()
            && !matches!(miss.failure, MissFailure::MethodSignatures(_))
        {
            miss.declaring = Some(DeclaringClass {
                unit: unit.clone(),
                index,
                required: Default::default(),
            });
        }
        error
    }

    pub(crate) fn property_signature(
        names: impl Iterator<Item = String>,
        failure: crate::error_traces::DeclarationFailure,
    ) -> NativeError {
        let mut names = names;
        let Some(name) = names.next() else {
            return NativeError::Unwind(Unwind(Box::new(failure)));
        };
        NativeError::Unwind(Unwind(Box::new(Self {
            name,
            failure: MissFailure::PropertySignature {
                remaining: names.collect(),
                failure,
            },
            declaring: None,
        })))
    }

    pub(crate) fn error(self) -> NativeError {
        match self.failure {
            MissFailure::Lookup(kind) => {
                NativeError::error(format!("{kind} \"{}\" not found", self.name))
            }
            MissFailure::Member(kind, member) => crate::unsupported_items::omitted_member(
                kind, &self.name, &member,
            )
            .unwrap_or_else(|| NativeError::error(format!("Class \"{}\" not found", self.name))),
            MissFailure::MethodSignatures(pending) => pending.methods.first().map_or_else(
                || internal_error!("empty method variance obligation"),
                |method| method.error(method.names.first().unwrap_or(&self.name)),
            ),
            MissFailure::PropertySignature { failure, .. } => {
                NativeError::Unwind(Unwind(Box::new(failure)))
            }
        }
    }
}

impl Vm {
    pub(crate) fn has_autoloaders(&self) -> bool {
        !self.autoload.callbacks.is_empty()
    }

    /// This path runs only after the ordinary class table lookup has failed.
    #[cold]
    pub(crate) fn class_miss(name: &str, kind: &'static str) -> NativeError {
        let miss = ClassMiss {
            name: name.into(),
            failure: MissFailure::Lookup(kind),
            declaring: None,
        };
        NativeError::Unwind(Unwind(Box::new(miss)))
    }

    /// The member's policy matters only after autoload has declined the class.
    #[cold]
    pub(crate) fn class_member_miss(
        name: &str,
        kind: phera_source::deprecations::ItemKind,
        member: &str,
    ) -> NativeError {
        NativeError::Unwind(Unwind(Box::new(ClassMiss {
            name: name.into(),
            failure: MissFailure::Member(kind, member.into()),
            declaring: None,
        })))
    }

    /// Most lookups finish immediately. Each failed lookup gets one pass through the loaders.
    pub(crate) async fn lookup_class(
        &mut self,
        name: &str,
    ) -> Result<Option<Rc<Class>>, NativeError> {
        self.lookup_class_from(name, None).await
    }

    pub(crate) async fn lookup_class_from(
        &mut self,
        name: &str,
        origin: Option<(&str, &[Value])>,
    ) -> Result<Option<Rc<Class>>, NativeError> {
        if name.is_empty() || name == "\\" {
            return Ok(None);
        }
        if let Some(class) = self.classes.get(name) {
            return Ok(Some(class));
        }
        let callback_name = name.strip_prefix('\\').unwrap_or(name);
        if let Some(class) = self.load_bundled_class(callback_name).await? {
            return Ok(Some(class));
        }
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
        let result = self.call_autoloaders(callback_name, origin).await;
        self.autoload.active.remove(&key);
        result?;
        Ok(self.classes.get(name))
    }

    /// Read the live list after each call so removing or appending loaders takes effect.
    /// Explicit `spl_autoload_call()` also calls the first loader for an existing name.
    pub(crate) async fn call_autoloaders(
        &mut self,
        name: &str,
        origin: Option<(&str, &[Value])>,
    ) -> Result<(), NativeError> {
        if let Some((function, arguments)) = origin {
            self.native_callback_sites
                .push(crate::error_traces::NativeCallbackSite {
                    boundary: self.call_stack.depth(),
                    native: crate::error_traces::NativeCaller::Saved(NativeTrace::new(
                        function, arguments,
                    )),
                });
        }
        let result = self.call_autoloaders_in_order(name).await;
        if origin.is_some() {
            self.native_callback_sites.pop();
        }
        result
    }

    async fn call_autoloaders_in_order(&mut self, name: &str) -> Result<(), NativeError> {
        let mut position = 0;
        while let Some(slot) = self.autoload.callbacks.get(position) {
            let loader = slot.clone();
            position += 1;
            let Some(loader) = loader else { continue };
            self.invoke_resolved_callable(loader.callee, [Value::from(name)])
                .await?;
            if self
                .classes
                .get_lowercase(&name.to_ascii_lowercase())
                .is_some()
            {
                break;
            }
        }
        Ok(())
    }

    /// Resume the interrupted operation only after the class is linked. Operands were
    /// already evaluated into registers before the fetch instruction was entered.
    #[cold]
    pub(crate) async fn resume_class_fetch(&mut self, miss: ClassMiss) -> Result<(), NativeError> {
        let completed_declaration = matches!(miss.failure, MissFailure::MethodSignatures(_));
        self.resolve_class_miss(miss).await?;
        if completed_declaration {
            return Ok(());
        }
        let frame = self
            .call_stack
            .current_mut()
            .ok_or_else(|| internal_error!("class fetch frame"))?;
        let position = frame
            .ip()
            .checked_sub(1)
            .ok_or_else(|| internal_error!("class fetch instruction"))?;
        frame.set_ip(position);
        Ok(())
    }

    /// Loads what a synchronous step that unwound with `miss` was missing:
    /// the class, through the autoloaders, or the classes of a signature.
    /// The error PHP raises when one stays missing.
    pub(crate) async fn resolve_class_miss(
        &mut self,
        mut miss: ClassMiss,
    ) -> Result<(), NativeError> {
        if !self.error_handlers.pending.is_empty() {
            self.drain_error_handlers().await?;
        }
        if let MissFailure::MethodSignatures(pending) = miss.failure {
            return self.finish_pending_class(pending).await;
        }
        let Some(declaring) = miss.declaring.take() else {
            return self.load_class_miss(miss).await;
        };
        let depth = self.autoload.declaring.len();
        self.autoload.declaring.push(declaring);
        let result = self.load_class_miss(miss).await;
        let required = self
            .autoload
            .declaring
            .get(depth)
            .filter(|entry| entry.required.get())
            .and_then(|entry| {
                let declaration = entry.unit.classes.get(entry.index.index())?;
                Some((
                    declaration.name.clone(),
                    entry.unit.file.clone(),
                    declaration.line as usize,
                ))
            });
        self.autoload.declaring.truncate(depth);
        match (result, required) {
            (Err(error), Some((class, file, line))) => {
                Err(self.variance_autoload_failure(error, &class, None, file, line))
            }
            (result, _) => result,
        }
    }

    async fn load_class_miss(&mut self, miss: ClassMiss) -> Result<(), NativeError> {
        if let MissFailure::PropertySignature { remaining, .. } = &miss.failure {
            let mut attempted = HashSet::new();
            let names = std::iter::once(&miss.name).chain(remaining);
            for name in names.clone() {
                if attempted.insert(name.to_ascii_lowercase()) {
                    self.lookup_class(name).await?;
                }
            }
            if names.clone().any(|name| self.classes.get(name).is_none()) {
                return Err(miss.error());
            }
        } else if self.lookup_class(&miss.name).await?.is_none() {
            return Err(miss.error());
        }
        Ok(())
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

impl crate::snapshot::ForkCopy for RegisteredLoader {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        RegisteredLoader {
            callee: self.callee.fork_copy(fork),
            display: fork.value(&self.display),
        }
    }
}

impl crate::snapshot::ForkCopy for Autoloaders {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        Autoloaders {
            callbacks: self.callbacks.fork_copy(fork),
            extensions: self.extensions.clone(),
            active: Default::default(),
            declaring: Default::default(),
            pending: Default::default(),
        }
    }
}
