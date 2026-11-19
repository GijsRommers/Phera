//! Request-local values of settings that exist at only some PHP levels.
//!
//! ```php
//! echo ini_get('filter.max_filter_count'); // PHP 8.6
//! ```

use phera_extension_api::NativeResult;
use phera_value::{NoDiagnostics, PhpString, Value, to_php_string};
use std::collections::HashMap;

use crate::{Vm, intrinsics::Intrinsic};

#[derive(Default)]
struct IniValues(HashMap<PhpString, PhpString>);

impl Vm {
    pub(super) fn versioned_ini(
        &mut self,
        intrinsic: Intrinsic,
        name: &PhpString,
        arguments: &[Value],
    ) -> Option<NativeResult> {
        let name_str = std::str::from_utf8(name.as_bytes()).ok()?;
        if self.registry.ini_unavailable(name_str) {
            return Some(Ok(Value::Bool(false)));
        }
        let default = self.registry.ini_default(name_str)?;
        let values = &mut self.state.get_or_default::<IniValues>().0;
        let previous = values.get(name).cloned().unwrap_or_else(|| default.into());
        if matches!(intrinsic, Intrinsic::IniSet) {
            let value = arguments.get(1).cloned().unwrap_or_default();
            match to_php_string(&value, &mut NoDiagnostics) {
                Ok(text) => {
                    values.insert(name.clone(), text);
                }
                Err(error) => return Some(Err(error.into())),
            }
        }
        Some(Ok(Value::String(previous)))
    }
}
