//! `ini_set('serialize_precision', '3')` changes `serialize(1.2345)` to `d:1.23;`.

use phera_extension_api::serialization::SerializationSettings;
use phera_extension_api::{NativeResult, value};
use phera_value::Value;

use crate::Vm;
use crate::intrinsics::Intrinsic;

impl Vm {
    pub(crate) fn serialization_ini_setting(
        &mut self,
        name: &[u8],
        intrinsic: Intrinsic,
        arguments: &[Value],
    ) -> Option<NativeResult> {
        let precision = match name {
            b"serialize_precision" => true,
            b"unserialize_max_depth" => false,
            _ => return None,
        };
        Some(self.read_or_set_serialization_ini(precision, intrinsic, arguments))
    }

    fn read_or_set_serialization_ini(
        &mut self,
        precision: bool,
        intrinsic: Intrinsic,
        arguments: &[Value],
    ) -> NativeResult {
        let settings = self.state_mut().get_or_default::<SerializationSettings>();
        let entry = if precision {
            &mut settings.precision
        } else {
            &mut settings.depth
        };
        let old = entry.clone();
        if matches!(intrinsic, Intrinsic::IniSet) {
            let value = arguments.get(1).cloned().unwrap_or_default();
            let value = value::to_php_string(&value, &mut value::NoDiagnostics)?.to_string_lossy();
            let number = parse_prefix(&value);
            if precision {
                if number < -1 {
                    return Ok(Value::Bool(false));
                }
                settings.precision_digits = number as i32;
            } else {
                settings.max_depth = usize::try_from(number).unwrap_or(0);
            }
            *entry = value;
        }
        Ok(Value::from(old))
    }
}

fn parse_prefix(text: &str) -> i64 {
    let text = text.trim_start();
    let sign = usize::from(text.starts_with(['+', '-']));
    let digits = text.as_bytes()[sign..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    text[..sign + digits].parse().unwrap_or(0)
}
