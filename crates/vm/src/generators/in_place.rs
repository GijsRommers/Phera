//! A suspended generator can execute register-only instructions in its own window.
//!
//! ```php
//! foreach ((function () { for ($i = 0; $i < 3; ++$i) yield $i; })() as $value) {}
//! ```
//!
//! The scalar interpreter cannot call back into PHP. When it encounters an
//! instruction that needs the VM, the normal resume path installs the frame
//! at that instruction, with the input to the preceding yield already bound.

use phera_bytecode::{Instruction, Reg};
use phera_extension_api::{GeneratorOperation, NativeError, NativeResult, traversal::IteratorStep};
use phera_value::{Object, Value};

use crate::{
    Vm,
    calls::execution_loop::YIELD_AFTER_STEPS,
    fast::{ScalarStep, scalar_step},
};

use super::{Generator, State, state};

pub(crate) type Element = (Value, Option<Value>);

impl Vm {
    pub(crate) fn resume_generator_operation(
        &mut self,
        object: &Object,
        operation: &GeneratorOperation,
    ) -> Option<NativeResult> {
        match operation {
            GeneratorOperation::Send(value) => self.resume_in_place(object, value, |generator| {
                generator
                    .item
                    .as_ref()
                    .map_or(Value::Null, |item| item.value.deref().clone())
            }),
            GeneratorOperation::Method(IteratorStep::Advance)
            | GeneratorOperation::Cursor {
                step: IteratorStep::Advance,
                ..
            } => self.resume_in_place(object, &Value::Null, |_| Value::Null),
            _ => None,
        }
    }

    pub(crate) fn resume_generator_element(
        &mut self,
        object: &Object,
        with_key: bool,
    ) -> Option<Result<Element, NativeError>> {
        self.resume_in_place(object, &Value::Null, |generator| {
            let item = generator.item.as_ref();
            let value = item.map_or(Value::Null, |item| item.value.deref().clone());
            let key = if with_key {
                item.map(|item| item.key.clone())
            } else {
                None
            };
            (value, key)
        })
    }

    fn resume_in_place<T>(
        &mut self,
        object: &Object,
        sent: &Value,
        result: impl FnOnce(&Generator) -> T,
    ) -> Option<Result<T, NativeError>> {
        if self.pending_callbacks || self.generator_releases_pending() {
            return None;
        }
        match state(object, |generator| {
            if let Some(leaf) = generator.delegation_leaf() {
                let outcome = state(&leaf, |leaf| {
                    self.run_in_place(leaf, sent).map(|()| result(leaf))
                })?;
                if outcome.is_some() {
                    generator.first_yield = false;
                }
                return Ok(outcome);
            }
            Ok(self.run_in_place(generator, sent).map(|()| {
                generator.first_yield = false;
                result(generator)
            }))
        })
        .and_then(|result| result)
        {
            Ok(result) => result.map(Ok),
            Err(error) => Some(Err(error)),
        }
    }

    /// The cached delegate can be shared. It qualifies only while it still has
    /// a local yield; finishing or delegating again requires the iterative driver.
    fn run_in_place(&mut self, generator: &mut Generator, sent: &Value) -> Option<()> {
        // While a destructor is due somewhere, a simple instruction may
        // release its object: the regular driver, whose loop looks, runs.
        if generator.reference || self.object_ids.has_due() {
            return None;
        }
        let State::Yielded(frame, received) = &mut generator.state else {
            return None;
        };
        let instructions = self.code.instructions(frame.frame.function);
        let mut ip = frame.frame.ip();
        let registers = frame.local_registers()?;
        *registers.get_mut(received.index())? = Some(sent.clone());

        let mut work = 0;
        let mut executed = 0;
        let yielded = loop {
            let Some(&instruction) = instructions.get(ip) else {
                break None;
            };
            let mut next = ip + 1;
            if matches!(
                scalar_step(&instruction, registers, &mut next),
                ScalarStep::Complete | ScalarStep::Released
            ) {
                executed += 1;
                if next <= ip {
                    work += 1;
                }
                ip = next;
                if work < YIELD_AFTER_STEPS {
                    continue;
                }
                break None;
            }
            let Instruction::Yield { dst, key, value } = instruction else {
                break None;
            };
            let Some(key) = read_operand(key, registers) else {
                break None;
            };
            let Some(value) = read_operand(value, registers) else {
                break None;
            };
            if self.generator_releases_pending() {
                break None;
            }
            executed += 1;
            ip = next;
            break Some((dst, key, value.unwrap_or_default()));
        };
        self.executed_instructions += executed;
        frame.frame.set_ip(ip);
        if let Some((destination, key, value)) = yielded {
            *received = destination;
            generator.publish(key, value);
            Some(())
        } else {
            let old = std::mem::replace(&mut generator.state, State::Running);
            if let State::Yielded(frame, _) = old {
                generator.state = State::Ready(frame);
            }
            None
        }
    }
}

/// Absent operands are distinct from operands referring to an undefined local.
fn read_operand(operand: Option<Reg>, registers: &[Option<Value>]) -> Option<Option<Value>> {
    match operand {
        Some(register) => Some(Some(
            registers.get(register.index())?.as_ref()?.deref().clone(),
        )),
        None => Some(None),
    }
}
