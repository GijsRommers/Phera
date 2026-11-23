//! Native scope intrinsics copy values while preserving the caller's variable names.

use std::rc::Rc;

use phera_extension_api::NativeResult;
use phera_value::{ArrayKey, PhpArray, Value};

use crate::Vm;

impl Vm {
    /// Copy defined locals and dynamic variables without exporting their reference cells.
    pub(crate) fn defined_variables(&mut self) -> NativeResult {
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
    pub(crate) fn compact_variables(&mut self, arguments: Vec<Value>) -> NativeResult {
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
                Value::String(string) => {
                    let name = string.to_string_lossy();
                    let value = if self.in_global_scope() {
                        self.request_global_value(&name)
                            .map(|value| value.deref().clone())
                    } else {
                        self.current()
                            .and_then(|frame| frame.named_register(&name))
                            .and_then(|register| self.read_defined_local(register))
                    };
                    if let Some(value) = value {
                        array.insert(ArrayKey::String(string), value);
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
