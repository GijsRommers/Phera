//! Single native dimensions can read an offset directly out of a register.
//!
//! ```php
//! $rows[$index] = $row;
//! echo $rows[$index];
//! ```

use phera_bytecode::{PathBase, PathId, Reg, Step as PathStep};
use phera_value::Value;

use super::execution_loop::{Machine, Position, Step};
use crate::{classes::operations::NativeDimension, dimensions};

impl Machine<'_> {
    pub(super) fn fetch_native_dimension(
        &mut self,
        position: Position,
        destination: Reg,
        base: Reg,
        index: Reg,
    ) -> Step {
        let frame = position.frame.base();
        let slots = self.registers.tail_mut(frame);
        let result = (|| {
            let Value::Object(object) = slots.get(base.index())?.as_ref()? else {
                return None;
            };
            let offset = slots.get(index.index())?.as_ref()?;
            let native = dimensions::native(object, NativeDimension::Get)?.direct?;
            (native.read)(object, offset)
        })();
        if let Some(value) = result
            && let Some(slot) = slots.get_mut(destination.index())
        {
            *slot = Some(value);
            Step::Done(position.ip)
        } else {
            Step::Slow
        }
    }

    pub(super) fn write_dimension_path(
        &mut self,
        position: Position,
        path: PathId,
        source: Reg,
    ) -> Step {
        let Some(path) = self
            .code
            .function(position.frame.function)
            .paths
            .get(path.index())
        else {
            return Step::Slow;
        };
        let PathBase::Register(base) = path.base else {
            return Step::Slow;
        };
        let offset = match path.steps.as_slice() {
            [PathStep::Index(register)] => Some(*register),
            [PathStep::Append] => None,
            _ => return Step::Slow,
        };
        self.write_native_dimension(position, base, offset, source)
    }

    pub(super) fn write_native_dimension(
        &mut self,
        position: Position,
        base: Reg,
        index: Option<Reg>,
        source: Reg,
    ) -> Step {
        let slots = self.registers.tail_mut(position.frame.base());
        let Some(Value::Object(object)) = slots.get(base.index()).and_then(Option::as_ref) else {
            return Step::Slow;
        };
        let Some(value) = slots.get(source.index()).and_then(Option::as_ref) else {
            return Step::Slow;
        };
        let offset = match index {
            Some(index) => {
                let Some(offset) = slots.get(index.index()).and_then(Option::as_ref) else {
                    return Step::Slow;
                };
                offset
            }
            None => &Value::Null,
        };
        let Some(native) =
            dimensions::native(object, NativeDimension::Set).and_then(|native| native.direct)
        else {
            return Step::Slow;
        };
        if (native.write)(object, offset, value) {
            Step::Done(position.ip)
        } else {
            Step::Slow
        }
    }
}
