//! Method variance collects missing types before asking PHP autoloaders to resolve them.
//!
//! ```php
//! class Base { function one(): DateTime {} function two(): DateTime {} }
//! class Child extends Base { function one(): A {} function two(): B {} }
//! // Autoload both A and B before reporting the first unresolved method.
//! ```

use std::{cell::Cell, collections::HashSet, rc::Rc, sync::Arc};

use phera_bytecode::{ClassId, CompiledUnit};
use phera_extension_api::{NativeError, Unwind};

use crate::{
    Vm,
    classes::{Class, ClassIndex, Method, worker_classes::Dependency},
    error_traces::DeclarationFailure,
};

pub(crate) struct DeclaringClass {
    pub(crate) unit: Arc<CompiledUnit>,
    pub(crate) index: ClassId,
    pub(crate) required: Cell<bool>,
}

pub(crate) struct PendingClass {
    pub(crate) class: Rc<Class>,
    pub(crate) unit: Arc<CompiledUnit>,
    pub(crate) methods: Vec<MethodVariance>,
    pub(crate) requested: HashSet<String>,
}

pub(crate) struct MethodVariance {
    pub(crate) names: Vec<String>,
    pub(crate) child: String,
    pub(crate) base: String,
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) inherited: Method,
    pub(crate) implementation: Method,
    pub(crate) trait_scope: Option<String>,
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
    pub(crate) fn class_for_inheritance(&self, name: &str) -> Option<Rc<Class>> {
        self.classes.get(name).or_else(|| {
            let entry = self
                .autoload
                .pending
                .iter()
                .find(|entry| entry.class.name.eq_ignore_ascii_case(name))?;
            self.classes.discard_link_dependencies();
            Some(entry.class.clone())
        })
    }

    /// A cached link may have depended on a name remaining unavailable.
    pub(crate) fn inherits_from_pending_type(&self, dependencies: &[Dependency]) -> bool {
        let differs = |name: &str| {
            dependencies
                .iter()
                .any(|(key, number)| number.is_none() && key.eq_ignore_ascii_case(name))
        };
        self.autoload
            .pending
            .iter()
            .any(|entry| differs(&entry.class.name))
            || self
                .autoload
                .declaring
                .iter()
                .any(|entry| differs(&entry.unit.classes[entry.index.index()].name))
    }

    pub(crate) fn is_pending_class(&self, index: ClassIndex) -> bool {
        self.autoload
            .pending
            .iter()
            .any(|entry| entry.class.index == index)
    }

    pub(super) async fn finish_pending_class(
        &mut self,
        pending: PendingClass,
    ) -> Result<(), NativeError> {
        let index = pending.class.index;
        self.autoload.pending.push(pending);
        let result = self.complete_pending_classes(index).await;
        if result.is_err() {
            self.autoload
                .pending
                .retain(|entry| entry.class.index != index);
        }
        result
    }

    /// A dependent definition can supply ancestry to its parent before it
    /// becomes visible. Finish such parents before publishing the dependent.
    async fn complete_pending_classes(&mut self, index: ClassIndex) -> Result<(), NativeError> {
        let mut work = vec![index];
        while let Some(&index) = work.last() {
            let Some(position) = self
                .autoload
                .pending
                .iter()
                .position(|entry| entry.class.index == index)
            else {
                work.pop();
                continue;
            };
            let entry = &self.autoload.pending[position];
            if let Some(parent) = entry
                .class
                .parent
                .iter()
                .chain(&entry.class.interfaces)
                .find(|parent| self.is_pending_class(parent.index))
            {
                work.push(parent.index);
                continue;
            }
            // Mark this name before invoking PHP. A loader can itself
            // declare a class whose publication depends on this one.
            let next = entry
                .methods
                .iter()
                .flat_map(|method| &method.names)
                .find(|name| {
                    !entry.requested.contains(&name.to_ascii_lowercase())
                        && self.class_for_inheritance(name).is_none()
                })
                .cloned();
            if let Some(name) = next {
                let class = entry.class.clone();
                let unit = entry.unit.clone();
                self.autoload.pending[position]
                    .requested
                    .insert(name.to_ascii_lowercase());
                if let Err(error) = self.lookup_class(&name).await {
                    let line = class
                        .declaration
                        .and_then(|(_, index)| unit.classes.get(index.index()))
                        .map_or(0, |declaration| declaration.line as usize);
                    return Err(self.variance_autoload_failure(
                        error,
                        &class.name,
                        Some(&name),
                        unit.file.clone(),
                        line,
                    ));
                }
                continue;
            }
            let pending = self.autoload.pending.remove(position);
            for method in &pending.methods {
                if let Some(remaining) = self.check_contract(
                    &method.inherited,
                    &method.implementation,
                    method.trait_scope.as_deref(),
                    &pending.unit,
                )? && let Some(name) = remaining.names.first()
                {
                    return Err(remaining.error(name));
                }
            }
            self.classes.publish_delayed(pending.class);
            work.pop();
        }
        Ok(())
    }

    pub(super) fn variance_autoload_failure(
        &self,
        error: NativeError,
        class: &str,
        loading: Option<&str>,
        file: String,
        line: usize,
    ) -> NativeError {
        let NativeError::Unwind(payload) = &error else {
            return error;
        };
        let Some(thrown) = payload.0.downcast_ref::<crate::exceptions::Thrown>() else {
            return error;
        };
        let context = loading.map_or_else(
            || " with variance dependencies".to_owned(),
            |name| format!(", while autoloading {name}"),
        );
        let cause = crate::exception_rendering::describe_chain(
            &thrown.object,
            self.error_handlers.trace_string_limit,
        );
        NativeError::Unwind(Unwind(Box::new(DeclarationFailure {
            message: format!("During inheritance of {class}{context}: Uncaught {cause}"),
            file,
            line,
        })))
    }
}
