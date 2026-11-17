//! Native scope intrinsics copy values while preserving the caller's variable names.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::NativeResult;
use phera_value::{ArrayKey, PhpArray, Value};

use crate::Vm;

impl Vm {
    /// Copy defined locals and dynamic variables without exporting their reference cells.
    pub(crate) fn defined_variables(&mut self) -> NativeResult {
        let mut array = PhpArray::new();
        if let Some(frame) = self.current() {
            for (i, name) in frame.function(&self.code).locals.iter().enumerate() {
                if let Some(Some(value)) = frame.slots(&self.registers).get(i) {
                    array.insert(
                        ArrayKey::String(
                            frame
                                .unit(&self.code)
                                .names
                                .get(name.index())
                                .cloned()
                                .unwrap_or_default()
                                .into(),
                        ),
                        value.deref().clone(),
                    );
                }
            }
            for (name, register) in self.call_stack.extras[frame.depth].dynamic.iter() {
                if let Some(Some(value)) = frame.slots(&self.registers).get(register.0 as usize) {
                    array.insert(ArrayKey::String(name.clone().into()), value.deref().clone());
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
                    let register = self.current().and_then(|frame| {
                        frame
                            .function(&self.code)
                            .locals
                            .iter()
                            .position(|name_id| {
                                frame
                                    .unit(&self.code)
                                    .names
                                    .get(name_id.index())
                                    .is_some_and(|candidate| candidate == &name)
                            })
                            .map(|i| Reg(i as u16))
                            .or_else(|| {
                                self.call_stack.extras[frame.depth]
                                    .dynamic
                                    .get(&name)
                                    .copied()
                            })
                    });
                    if let Some(register) = register {
                        array.insert(ArrayKey::String(string), self.get(register));
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
