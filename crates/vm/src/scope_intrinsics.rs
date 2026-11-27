//! Native scope intrinsics copy values while preserving the caller's variable names.

use std::rc::Rc;

use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, PhpArray, Value};

use crate::Vm;

impl Vm {
    /// Copy defined locals and dynamic variables without exporting their reference cells.
    pub(crate) fn defined_variables(
        &mut self,
        mut arguments: Vec<Value>,
        dynamic: bool,
    ) -> NativeResult {
        if let Err(error) = self.prepare_call_with_trace("get_defined_vars", &[], &mut arguments) {
            self.failed_native = Some(crate::error_traces::NativeTrace::new(
                "get_defined_vars",
                &arguments,
            ));
            return Err(error);
        }
        if dynamic {
            self.failed_native = Some(crate::error_traces::NativeTrace::new(
                "get_defined_vars",
                &arguments,
            ));
            return Err(NativeError::error(
                "Cannot call get_defined_vars() dynamically",
            ));
        }
        let mut array = PhpArray::new();
        if self.in_global_scope() {
            for (name, value) in self.globals.iter() {
                array.insert(ArrayKey::str(name.as_str()), value.deref().clone());
            }
        }
        if let Some(frame) = self.current() {
            for (name, register) in frame.named_registers() {
                if let Some(Some(value)) = frame.slots(&self.registers).get(register.index()) {
                    array.insert(ArrayKey::str(name.as_str()), value.deref().clone());
                }
            }
        }
        Ok(Value::Array(Rc::new(array)))
    }

    /// Walk compact names iteratively so nested arrays do not consume the Rust stack.
    pub(crate) fn compact_variables(
        &mut self,
        arguments: Vec<Value>,
        dynamic: bool,
    ) -> NativeResult {
        if arguments.is_empty() {
            self.failed_native = Some(crate::error_traces::NativeTrace::new("compact", &arguments));
            return Err(NativeError::argument_count_error(
                "compact() expects at least 1 argument, 0 given",
            ));
        }
        if dynamic {
            self.failed_native = Some(crate::error_traces::NativeTrace::new("compact", &arguments));
            return Err(NativeError::error("Cannot call compact() dynamically"));
        }
        let mut names: Vec<_> = arguments.into_iter().rev().collect();
        let mut array = PhpArray::new();
        while let Some(value) = names.pop() {
            match value {
                Value::Array(values) => names.extend(
                    values
                        .values()
                        .cloned()
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev(),
                ),
                Value::InternedStr(_) | Value::String(_) => {
                    let s = value.str_ref().expect("checked string");
                    let name = s.to_string_lossy();
                    let found = if self.in_global_scope() {
                        self.request_global_value(&name)
                            .map(|value| value.deref().clone())
                    } else {
                        self.current()
                            .and_then(|frame| frame.named_register(&name))
                            .and_then(|register| self.read_defined_local(register))
                    };
                    if let Some(found) = found {
                        let key = match value {
                            Value::InternedStr(s) => ArrayKey::Interned(s),
                            Value::String(s) => ArrayKey::String(s),
                            _ => unreachable!(),
                        };
                        array.insert(key, found);
                    } else {
                        self.warn(format!("compact(): Undefined variable ${name}"));
                    }
                }
                _ => {}
            }
        }
        Ok(Value::Array(Rc::new(array)))
    }
}
