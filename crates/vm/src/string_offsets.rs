//! PHP string offset reads use byte positions, including negative offsets.

use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, PhpString, Value};

use crate::{Vm, access_mode::Missing};

impl Vm {
    /// One byte of a string, as `$s[2]` or `$s[-1]`.
    ///
    /// What a position outside the string gives depends on who asks:
    ///
    /// ```php
    /// $s = "abc";
    /// echo $s[3];          // "" and "Warning: Uninitialized string offset 3"
    /// isset($s[3]);        // false
    /// $s[3] ?? 'default';  // 'default'
    /// isset($s["x"]);      // false: not a position at all
    /// echo $s["x"];        // TypeError
    /// ```
    ///
    /// So for `isset`, `empty` and `??` the answer is null, which is how
    /// they see "not there".
    pub(crate) fn read_string_offset(
        &mut self,
        string: &PhpString,
        key: ArrayKey,
        missing: Missing,
    ) -> NativeResult {
        let asking_whether_it_exists = missing != Missing::Warn;
        let index = match key {
            ArrayKey::Int(index) => index,
            ArrayKey::String(key) => match key.to_string_lossy().parse::<i64>() {
                Ok(index) => index,
                // `isset` also takes an integer with spaces around it
                // (`" 1"`), as PHP's numeric strings allow.
                Err(_) if asking_whether_it_exists => match whole_integer(key.as_bytes()) {
                    Some(index) => index,
                    None => return Ok(Value::Null),
                },
                Err(_) => {
                    return Err(NativeError::type_error(
                        "Cannot access offset of type string on string",
                    ));
                }
            },
        };
        let from_start = if index < 0 {
            string.len() as i64 + index
        } else {
            index
        };
        let byte = usize::try_from(from_start)
            .ok()
            .and_then(|position| string.as_bytes().get(position));
        Ok(match byte {
            Some(byte) => Value::from(vec![*byte]),
            None if asking_whether_it_exists => Value::Null,
            None => {
                self.warn(format!("Uninitialized string offset {index}"));
                Value::from("")
            }
        })
    }
}

/// The integer a numeric string stands for, when all of it is one integer:
/// `" 12"` and `"12 "` are 12; `"12x"`, `"1.5"` and `""` are none.
fn whole_integer(bytes: &[u8]) -> Option<i64> {
    let parsed = phera_value::parse_numeric(bytes);
    match parsed.value {
        Some(phera_value::NumVal::Int(index)) if parsed.exact => Some(index),
        _ => None,
    }
}
