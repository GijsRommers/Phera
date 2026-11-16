//! Resolved signatures and pending arguments describe a PHP call before it executes.

use std::rc::Rc;

use phera_extension_api::FunctionEntry;
use phera_value::Value;

/// A signature and target resolved once for a symbol in this request.
#[derive(Clone)]
pub(crate) struct Callee {
    pub(crate) name: String,
    pub(crate) parameters: Vec<ParameterSpec>,
    pub(crate) target: CallTarget,
}

/// Named argument routing and PHP parameter binding modifiers.
#[derive(Clone)]
pub(crate) struct ParameterSpec {
    pub(crate) name: String,
    pub(crate) by_reference: bool,
    pub(crate) variadic: bool,
}

/// User bytecode, a native entry, or an intrinsic resolved at the call site.
#[derive(Clone)]
pub(crate) enum CallTarget {
    /// Bound methods retain lexical and called scopes throughout argument evaluation.
    Method {
        method: Rc<crate::classes::Method>,
        context: crate::classes::MethodContext,
    },
    /// A class without a constructor still consumes sends and returns the allocated handle.
    Constructed(Rc<phera_value::Object>),
    User(crate::function_table::FunctionRef),
    Native(FunctionEntry),
    Intrinsic(crate::intrinsics::Intrinsic),
}

/// An unknown named argument belongs to a variadic collector and retains its string key.
pub(crate) struct NamedArgument {
    pub(crate) name: String,
    pub(crate) value: Value,
}

/// Extra argument storage is allocated only for calls using a named variadic collector.
pub(crate) struct NamedArguments {
    pub(crate) values: Vec<NamedArgument>,
}

/// Once a named argument is seen, further unpacked integer keys are illegal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgumentOrder {
    Positional,
    Named,
}

/// An unfinished call; nested argument calls push another entry.
pub(crate) struct PendingCall {
    pub(crate) callee: Rc<Callee>,
    pub(crate) arguments: Vec<Option<Value>>,
    pub(crate) named: Option<String>,
    pub(crate) extra_named: Option<Box<NamedArguments>>,
}

impl PendingCall {
    /// Route extra names into the variadic array while rejecting overwritten names.
    #[inline]
    pub(crate) fn store_argument(
        &mut self,
        index: usize,
        named: Option<String>,
        value: Value,
    ) -> Result<(), phera_extension_api::NativeError> {
        if named.is_none() {
            self.arguments.push(Some(value));
            return Ok(());
        }
        self.extra_named
            .get_or_insert_with(|| Box::new(NamedArguments { values: Vec::new() }));
        if let Some(name) = &named
            && !self
                .callee
                .parameters
                .iter()
                .any(|parameter| !parameter.variadic && &parameter.name == name)
        {
            if self
                .extra_named
                .iter()
                .flat_map(|arguments| arguments.values.iter())
                .any(|argument| &argument.name == name)
            {
                return Err(phera_extension_api::NativeError::error(format!(
                    "Named parameter ${name} overwrites previous argument"
                )));
            }
            self.extra_named
                .get_or_insert_with(|| Box::new(NamedArguments { values: Vec::new() }))
                .values
                .push(NamedArgument {
                    name: name.clone(),
                    value,
                });
        } else {
            if self.arguments.get(index).is_some_and(Option::is_some) {
                return Err(phera_extension_api::NativeError::error(format!(
                    "Named parameter ${} overwrites previous argument",
                    named.unwrap_or_default()
                )));
            }
            self.arguments
                .resize_with(self.arguments.len().max(index + 1), || None);
            self.arguments[index] = Some(value);
        }
        self.named = None;
        Ok(())
    }

    /// Named ordering is encoded by lazily allocated metadata, keeping positional calls small.
    pub(crate) fn order(&self) -> ArgumentOrder {
        if self.extra_named.is_some() {
            ArgumentOrder::Named
        } else {
            ArgumentOrder::Positional
        }
    }

    /// Named sends select a signature slot; positional sends follow prepared arguments.
    pub(crate) fn argument_index(&self) -> Result<usize, phera_extension_api::NativeError> {
        if let Some(named) = &self.named {
            self.callee
                .parameters
                .iter()
                .position(|parameter| &parameter.name == named && !parameter.variadic)
                .or_else(|| {
                    self.callee
                        .parameters
                        .iter()
                        .position(|parameter| parameter.variadic)
                })
                .ok_or_else(|| {
                    phera_extension_api::NativeError::error(format!(
                        "Unknown named parameter ${named}"
                    ))
                })
        } else {
            Ok(self.arguments.len())
        }
    }
}

/// Signature metadata is built together with the resolved execution target.
pub(crate) struct ResolvedSignature {
    pub(crate) target: CallTarget,
    pub(crate) parameters: Vec<ParameterSpec>,
}
