//! `$object->items['key'] = $source` reads the local after it resolves the write.
//! An overloaded setter can change the assignment result by updating the local.

use phera_bytecode::Reg;
use phera_value::Value;

use crate::Vm;

pub(crate) struct WriteValue {
    value: Value,
    local: Option<AssignmentLocal>,
}

#[derive(Clone, Copy)]
pub(super) struct AssignmentLocal {
    source: Reg,
    destination: Reg,
}

impl WriteValue {
    pub(crate) fn evaluated(value: Value) -> Self {
        Self { value, local: None }
    }

    pub(crate) fn local(source: Reg, destination: Reg) -> Self {
        Self {
            value: Value::Null,
            local: Some(AssignmentLocal {
                source,
                destination,
            }),
        }
    }

    /// Read a local only when the write reaches the final destination.
    pub(super) fn read(&mut self, vm: &mut Vm) -> &Value {
        if let Some(local) = self.local.take() {
            self.value = vm.read_local(local.source);
            vm.set(local.destination, self.value.clone());
        }
        &self.value
    }

    pub(crate) fn into_value(mut self, vm: &mut Vm) -> Value {
        self.read(vm);
        self.value
    }
}

impl WriteValue {
    pub(super) fn pending_local(&self) -> Option<AssignmentLocal> {
        self.local
    }
}

impl AssignmentLocal {
    /// __set receives a copy, while the result reads the local after the call.
    pub(super) fn refresh_result(self, vm: &mut Vm) {
        let value = vm.read_defined_local(self.source).unwrap_or_default();
        vm.set(self.destination, value);
    }
}
