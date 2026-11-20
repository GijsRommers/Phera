//! `xdebug_break()` asks an attached debugger to stop at the next PHP instruction.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Registry};
use phera_value::Value;

use crate::Vm;

pub(crate) fn register(registry: &mut Registry) {
    registry.sapi_ini("phera_debugger");
    registry.function(FunctionEntry::sync("xdebug_break", &[], break_now));
    registry.describe_function("xdebug_break", "bool", None);
}

fn break_now(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Bool(context.break_into_debugger()))
}

impl Vm {
    pub(crate) fn break_into_debugger(&mut self) -> bool {
        if let Some(session) = &mut self.debugger {
            session.force_break = true;
            true
        } else {
            false
        }
    }
}
