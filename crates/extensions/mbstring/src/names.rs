//! What the encodings are called:
//!
//! ```php
//! mb_list_encodings();               // ["7bit", "8bit", "UCS-4", ...]
//! mb_encoding_aliases('ASCII');      // ["ANSI_X3.4-1968", "iso-ir-6", ...]
//! mb_preferred_mime_name('sjis');    // "Shift_JIS"
//! ```

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value, args,
};

use crate::encoding::{self, Encoding};

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "mb_list_encodings",
        &[],
        mb_list_encodings,
    ));
    registry.function(FunctionEntry::sync(
        "mb_encoding_aliases",
        ONE_ENCODING,
        mb_encoding_aliases,
    ));
    registry.function(FunctionEntry::sync(
        "mb_preferred_mime_name",
        ONE_ENCODING,
        mb_preferred_mime_name,
    ));
}

/// A PHP list of the values.
pub fn list(values: impl IntoIterator<Item = Value>) -> Value {
    let mut array = PhpArray::new();
    for value in values {
        // Pushing only fails once the next integer key would overflow,
        // which a fresh list cannot reach.
        let _ = array.push(value);
    }
    Value::from(array)
}

/// `mb_list_encodings(): array`
fn mb_list_encodings(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(list(
        Encoding::ALL
            .iter()
            .map(|encoding| Value::from(encoding.name())),
    ))
}

const ONE_ENCODING: &[Param] = &[Param::new("encoding", ParamType::String)];

/// `mb_encoding_aliases(string $encoding): array`
fn mb_encoding_aliases(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let encoding = given(ctx, a, "mb_encoding_aliases")?;
    Ok(list(
        encoding.aliases().iter().map(|&alias| Value::from(alias)),
    ))
}

/// `mb_preferred_mime_name(string $encoding): string|false`
fn mb_preferred_mime_name(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let encoding = given(ctx, a, "mb_preferred_mime_name")?;
    Ok(Value::from(encoding.mime_name()))
}

fn given(
    ctx: &mut dyn Context,
    a: &[Value],
    function: &str,
) -> Result<Encoding, phera_extension_api::NativeError> {
    let name = String::from_utf8_lossy(args::bytes(a, 0));
    encoding::named(ctx, &name, function, 1, "encoding")
}
