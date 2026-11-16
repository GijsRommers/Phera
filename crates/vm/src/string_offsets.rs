//! PHP string offset reads use byte positions, including negative offsets.

use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, PhpString, Value};

use crate::{Vm, access_mode::Missing};

impl Vm {
    /// A missing byte yields an empty string; ordinary reads also warn.
    pub(crate) fn read_string_offset(
        &mut self,
        string: &PhpString,
        key: ArrayKey,
        missing: Missing,
    ) -> NativeResult {
        let index = match key {
            ArrayKey::Int(index) => index,
            ArrayKey::String(key) => {
                let name = key.to_string_lossy();
                name.parse::<i64>().map_err(|_| {
                    NativeError::type_error("Cannot access offset of type string on string")
                })?
            }
        };
        let index = if index < 0 {
            string.len() as i64 + index
        } else {
            index
        };
        let value = if let Some(byte) = usize::try_from(index)
            .ok()
            .and_then(|index| string.as_bytes().get(index))
        {
            Value::from(vec![*byte])
        } else {
            if missing == Missing::Warn {
                self.warn(format!("Uninitialized string offset {index}"))
            }
            Value::from("")
        };
        Ok(value)
    }
}
