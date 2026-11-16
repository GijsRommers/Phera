mod case;
mod replace;
mod search;
mod split;
mod trim;
use phera_extension_api::*;
pub(crate) fn register(r: &mut Registry) {
    case::register(r);
    replace::register(r);
    trim::register(r);
    split::register(r);
    search::register(r);
}
pub(super) fn bytes(v: impl Into<Vec<u8>>) -> Value {
    Value::String(PhpString::from(v.into()))
}
pub(super) fn buffer(len: usize) -> Result<Vec<u8>, NativeError> {
    if len > 2 * 1024 * 1024 * 1024 {
        return Err(NativeError::error(
            "String size exceeds the maximum allowed size",
        ));
    }
    let mut v = Vec::new();
    v.try_reserve_exact(len)
        .map_err(|_| NativeError::error("Could not allocate string"))?;
    Ok(v)
}
