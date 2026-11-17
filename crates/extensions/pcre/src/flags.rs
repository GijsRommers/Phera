//! Flags affect shape: `preg_match('/(a)?/', '', $m, PREG_UNMATCHED_AS_NULL)`.

use phera_extension_api::NativeError;
#[derive(Clone, Copy, Default)]
pub(crate) struct CaptureFlags {
    pub offsets: bool,
    pub unmatched_null: bool,
}
impl CaptureFlags {
    pub(crate) fn parse(value: i64) -> Self {
        Self {
            offsets: value & 256 != 0,
            unmatched_null: value & 512 != 0,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Order {
    Single,
    Pattern,
    Set,
}
pub(crate) fn match_flags(
    name: &str,
    value: i64,
    global: bool,
) -> Result<(Order, CaptureFlags), NativeError> {
    let order = match (global, value & 255) {
        (false, 0) => Order::Single,
        (true, 0 | 1) => Order::Pattern,
        (true, 2) => Order::Set,
        _ => {
            return Err(NativeError::value_error(format!(
                "{name}(): Argument #4 ($flags) must be a PREG_* constant"
            )));
        }
    };
    Ok((order, CaptureFlags::parse(value)))
}
pub(crate) struct SplitFlags {
    pub no_empty: bool,
    pub delimiters: bool,
    pub offsets: bool,
}
impl SplitFlags {
    pub(crate) fn parse(value: i64) -> Self {
        Self {
            no_empty: value & 1 != 0,
            delimiters: value & 2 != 0,
            offsets: value & 4 != 0,
        }
    }
}
