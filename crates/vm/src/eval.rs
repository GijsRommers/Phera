//! `$result = eval('$count++; return $count;');` runs a new unit in the caller's scope.

use std::sync::Arc;

use phera_bytecode::Reg;
use phera_extension_api::NativeError;

use crate::Vm;

impl Vm {
    /// Evaluated units belong to this request and use the same frame protocol as includes.
    pub(crate) async fn eval(&mut self, destination: Reg, source: Reg) -> Result<(), NativeError> {
        // A strict image holds every unit the program may run; code compiled
        // from a string would not be one of them.
        if self
            .state
            .get::<phera_extension_api::ImageSeal>()
            .is_some_and(|seal| seal.forbid_eval)
        {
            return Err(self.sealed_eval_error());
        }
        // The code keeps its bytes: a string literal inside it may hold any.
        let source = self
            .stringify_value(&self.get(source))
            .await?
            .as_bytes()
            .to_vec();
        if source.is_empty() {
            self.set(destination, phera_value::Value::Bool(false));
            return Ok(());
        }
        let location = self.location();
        let file = format!("{}({}) : eval()'d code", location.file, location.line);
        // Ordinary files count occurrences from zero. Give each evaluated
        // unit its own range even when two executions share the source line.
        let anonymous_seed = u64::from(self.code.next_unit_index().0) << 32;
        let debug_source = self.debugger.as_ref().map(|_| source.clone());
        let compiled = tokio::task::spawn_blocking(move || {
            phera_compiler::compile_eval_with_seed(&source, &file, anonymous_seed)
        })
        .await
        .map_err(|error| NativeError::error(format!("Eval compiler failed: {error}")))?;
        match compiled {
            Ok(unit) if debug_source.is_some() => {
                let unit = Arc::new(unit);
                self.enter_scoped_unit(unit.clone(), Some(destination))?;
                if let (Some(debugger), Some(source)) = (&mut self.debugger, debug_source) {
                    debugger.sources.remember(&unit, source);
                }
                Ok(())
            }
            Ok(unit) => self.enter_scoped_unit(Arc::new(unit), Some(destination)),
            Err(error) => Err(self.dynamic_compile_failure(error).await),
        }
    }

    /// The runtime registers `Phera\SealedImageException`; a VM embedded
    /// without it reports the same message as a plain `Error`.
    #[cold]
    fn sealed_eval_error(&self) -> NativeError {
        const SEALED: &str = "Phera\\SealedImageException";
        let message = "eval() is disabled in strict AOT image mode";
        if self.classes.get_ref(SEALED).is_some() {
            NativeError::throw(SEALED, message)
        } else {
            NativeError::error(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use std::sync::Arc;

    use phera_extension_api::{
        ClassEntry, Context, ImageSeal, MethodEntry, NativeResult, Registry,
    };
    use phera_value::{Object, Value};

    use crate::Vm;

    fn message(
        _context: &mut dyn Context,
        object: Option<&Rc<Object>>,
        _arguments: &[Value],
    ) -> NativeResult {
        Ok(object
            .and_then(|object| object.get(&Object::protected_key("message")))
            .unwrap_or_default())
    }

    /// `Error`, and the sealed-image exception when the embedder has one.
    fn registry(with_sealed_class: bool) -> Registry {
        let mut registry = Registry::new();
        registry.class(ClassEntry::interface("Throwable"));
        registry.class(ClassEntry {
            interfaces: &["Throwable"],
            methods: const { &[MethodEntry::public("getMessage", &[], message)] },
            ..ClassEntry::class("Error")
        });
        if with_sealed_class {
            registry.class(ClassEntry {
                parent: Some("Error"),
                ..ClassEntry::class("Phera\\SealedImageException")
            });
        }
        registry
    }

    const SCRIPT: &str = r#"<?php
        try {
            echo eval('return 21 * 2;');
        } catch (Throwable $error) {
            echo get_class($error), ': ', $error->getMessage();
        }
    "#;

    async fn run(registry: Registry, seal: Option<ImageSeal>) -> String {
        let unit = phera_compiler::compile(SCRIPT, "eval-seal.php").unwrap();
        let mut vm = Vm::new(Arc::new(registry));
        if let Some(seal) = seal {
            *vm.state_mut().get_or_default::<ImageSeal>() = seal;
        }
        assert_eq!(vm.run(Arc::new(unit)).await, 0);
        String::from_utf8(vm.output.clone()).unwrap()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn eval_runs_unless_the_request_is_sealed() {
        assert_eq!(run(registry(true), None).await, "42");
        let unsealed = ImageSeal::default();
        assert_eq!(run(registry(true), Some(unsealed)).await, "42");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_sealed_request_throws_instead_of_compiling() {
        let sealed = ImageSeal {
            forbid_eval: true,
            forbid_class_alias: false,
        };
        assert_eq!(
            run(registry(true), Some(sealed)).await,
            "Phera\\SealedImageException: eval() is disabled in strict AOT image mode"
        );
        // Without the runtime's exception class the same message is an Error.
        assert_eq!(
            run(registry(false), Some(sealed)).await,
            "Error: eval() is disabled in strict AOT image mode"
        );
    }
}
