//! Synchronous instruction handlers keep PHP semantics inside the shared frame loop.

use crate::{Vm, fast::ScalarStep};
use phera_bytecode::Instruction;
use phera_extension_api::NativeError;

use phera_value::{PhpArray, Value};
use std::rc::Rc;

impl Vm {
    /// Object instructions preserve lexical scope while delegating their storage rules.
    #[inline]
    pub(crate) fn object_instruction(
        &mut self,
        instruction: Instruction,
    ) -> Result<ScalarStep, NativeError> {
        match instruction {
            Instruction::DeclareClass { class } => self.declare_class(class)?,
            Instruction::DynamicInstanceOf { dst, source, class } => {
                self.dynamic_instance_of(dst, source, class)?
            }
            Instruction::FetchClassConstant { dst, class, name } => {
                let class = self.class_target(class)?;
                let name = self.name(name)?;
                let value = self.class_constant(&class, &name)?;
                self.set(dst, value);
            }
            Instruction::FetchClassName { dst, class } => {
                let name = self.class_symbol_value(class)?;
                self.set(dst, name);
            }
            Instruction::FetchThis { dst } => self.fetch_this(dst)?,
            Instruction::Clone { dst, src } => self.clone_object(dst, src)?,
            Instruction::InstanceOf { dst, src, class } => self.instance_of(dst, src, class)?,
            Instruction::ThisOrNull { dst } => {
                let value = self
                    .current()
                    .and_then(|frame| frame.method.as_ref())
                    .and_then(|context| context.receiver.clone())
                    .map(Value::Object)
                    .unwrap_or_default();
                self.set(dst, value);
            }
            _ => return Ok(ScalarStep::Fallback),
        }
        Ok(ScalarStep::Complete)
    }
    /// Arrays and closures create PHP values through their ordinary allocation handlers.
    #[inline]
    pub(crate) fn array_instruction(
        &mut self,
        instruction: Instruction,
    ) -> Result<ScalarStep, NativeError> {
        match instruction {
            Instruction::Capture { src, kind } => self.capture(src, kind)?,
            Instruction::MakeClosure {
                dst,
                function,
                binding,
            } => {
                self.make_closure(dst, function, binding)?;
            }
            Instruction::NewArray {
                dst: destination, ..
            } => self.set(destination, Value::Array(Rc::new(PhpArray::new()))),
            Instruction::ArrayPush { array, src: source }
            | Instruction::ArrayPushReference { array, src: source } => {
                self.array_write(array, None, source, crate::arrays::ArrayWrite::Element)?
            }
            Instruction::ArraySet {
                array,
                key,
                src: source,
            }
            | Instruction::ArraySetReference {
                array,
                key,
                src: source,
            } => self.array_write(array, Some(key), source, crate::arrays::ArrayWrite::Element)?,
            Instruction::ArraySpread { array, src: source } => {
                self.array_write(array, None, source, crate::arrays::ArrayWrite::Spread)?
            }
            _ => return Ok(ScalarStep::Fallback),
        }
        Ok(ScalarStep::Complete)
    }
    /// Foreach operations own and release their compiler-assigned iterator slots.
    #[inline]
    pub(crate) fn iteration_instruction(
        &mut self,
        instruction: Instruction,
    ) -> Result<ScalarStep, NativeError> {
        match instruction {
            Instruction::IterStart {
                iter: iterator,
                src: source,
                ..
            } => self.iter_start(iterator, source, None)?,
            Instruction::IterStartReference {
                subject,
                iter: iterator,
                path,
                ..
            } => self.start_reference_iterator(iterator, path, subject)?,
            Instruction::IterNextKeyed {
                iter: iterator,
                key,
                value,
                end,
            } => self.iter_next(iterator, Some(key), value, end)?,
            Instruction::IterNext {
                iter: iterator,
                value,
                end,
            } => self.iter_next(iterator, None, value, end)?,
            Instruction::IterEnd { iter: iterator } => self.end_iterator(iterator)?,
            _ => return Ok(ScalarStep::Fallback),
        }
        Ok(ScalarStep::Complete)
    }
}
