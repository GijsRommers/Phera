//! Method variance collects missing types before asking PHP autoloaders to resolve them.
//!
//! ```php
//! class Base { function one(): DateTime {} function two(): DateTime {} }
//! class Child extends Base { function one(): A {} function two(): B {} }
//! // Autoload both A and B before reporting the first unresolved method.
//! ```

use std::collections::HashSet;

use phera_extension_api::{NativeError, Unwind};

use crate::{Vm, error_traces::DeclarationFailure};

pub(crate) struct MethodVariance {
    pub(crate) names: Vec<String>,
    pub(crate) child: String,
    pub(crate) base: String,
    pub(crate) file: String,
    pub(crate) line: usize,
}

impl MethodVariance {
    pub(super) fn error(&self, name: &str) -> NativeError {
        NativeError::Unwind(Unwind(Box::new(DeclarationFailure {
            message: format!(
                "Could not check compatibility between {} and {}, because class {name} is not available",
                self.child, self.base,
            ),
            file: self.file.clone(),
            line: self.line,
        })))
    }
}

impl Vm {
    pub(super) async fn load_method_signature_classes(
        &mut self,
        methods: &[MethodVariance],
    ) -> Result<(), NativeError> {
        let mut attempted = HashSet::new();
        for method in methods {
            for name in &method.names {
                if attempted.insert(name.to_ascii_lowercase()) {
                    self.lookup_class(name).await?;
                }
            }
        }
        for method in methods {
            if let Some(name) = method
                .names
                .iter()
                .find(|name| self.classes.get(name).is_none())
            {
                return Err(method.error(name));
            }
        }
        Ok(())
    }
}
