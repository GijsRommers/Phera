//! Read image type and dimensions without allocating pixel storage, as in `getimagesize($path)`.

mod headers;
mod kind;

use phera_ext_streams::native::{StreamContents, inspect_path};
use phera_extension_api::{
    ArrayKey, ConstantValue as C, Context, Extension, FunctionEntry as F, LocalFuture, NativeError,
    NativeResult, Param as P, ParamType as T, PhpArray, Registry, Value, args,
};

use self::kind::Kind;

pub struct ImageMetadata;
pub struct ExifImageType;

const INPUT: P = P::new("filename", T::String).without_nul();
const BUFFER: P = P::new("string", T::String);
const INFO: P = P::new("image_info", T::Untyped)
    .by_ref()
    .with_default(C::Null);
const TYPE: P = P::new("image_type", T::Int);
const DOT: P = P::new("include_dot", T::Bool).with_default(C::Bool(true));
const LIMIT: u64 = 8 * 1024 * 1024;

impl Extension for ImageMetadata {
    fn name(&self) -> &'static str {
        "image-metadata"
    }

    fn php_module(&self) -> Option<&'static str> {
        Some("standard")
    }

    fn register(&self, registry: &mut Registry) {
        kind::register(registry);
        for (entry, result) in const {
            [
                (
                    F::asynchronous("getimagesize", &[INPUT, INFO], from_file),
                    "array|false",
                ),
                (
                    F::sync("getimagesizefromstring", &[BUFFER, INFO], from_string),
                    "array|false",
                ),
                (
                    F::sync("image_type_to_extension", &[TYPE, DOT], extension),
                    "string|false",
                ),
                (F::sync("image_type_to_mime_type", &[TYPE], mime), "string"),
            ]
        } {
            registry.function(entry);
            registry.describe_function(entry.name, result, None);
        }
    }
}

impl Extension for ExifImageType {
    fn name(&self) -> &'static str {
        "exif"
    }

    fn register(&self, registry: &mut Registry) {
        registry.function(F::asynchronous("exif_imagetype", &[INPUT], exif_type));
        registry.describe_function("exif_imagetype", "int|false", None);
    }
}

fn mime(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    Ok(Value::from(
        Kind::from_id(args::int(args, 0).unwrap_or(0))
            .map_or("application/octet-stream", Kind::mime),
    ))
}

fn extension(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let dot = args::bool(args, 1).unwrap_or(true);
    Ok(
        Kind::from_id(args::int(args, 0).unwrap_or(0)).map_or(Value::Bool(false), |format| {
            let ext = format.extension();
            Value::from(if dot { ext } else { &ext[1..] })
        }),
    )
}

fn from_string(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let data = args::bytes(arguments, 0);
    measure(context, arguments, data, data, "getimagesizefromstring")
}

fn from_file(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        const NAME: &str = "getimagesize";
        let name = args::bytes(&arguments, 0);
        if name.is_empty() {
            return Err(NativeError::value_error("Path must not be empty"));
        }
        clear_information(&arguments);
        let Some(data) = read_path(context, name, NAME).await? else {
            return Ok(Value::Bool(false));
        };
        measure(context, &arguments, &data, name, NAME)
    })
}

fn exif_type(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        const NAME: &str = "exif_imagetype";
        let name = args::bytes(&arguments, 0);
        if name.is_empty() {
            return Err(NativeError::value_error("Path must not be empty"));
        }
        let Some(data) = read_path(context, name, NAME).await? else {
            return Ok(Value::Bool(false));
        };
        Ok(detect(context, &data, name, NAME)
            .map_or(Value::Bool(false), |ty| Value::Int(ty as i64)))
    })
}

async fn read_path(
    context: &mut dyn Context,
    name: &[u8],
    function: &str,
) -> Result<Option<Vec<u8>>, NativeError> {
    Ok(
        match inspect_path(context, function, name, LIMIT, 0).await? {
            Some(StreamContents::Bytes { prefix, .. }) => Some(prefix),
            Some(StreamContents::Directory) => Some(Vec::new()),
            None => None,
        },
    )
}

fn clear_information(arguments: &[Value]) {
    if let Some(info) = args::reference(arguments, 1) {
        *info.borrow_mut() = Value::from(PhpArray::new());
    }
}

fn measure(
    context: &mut dyn Context,
    arguments: &[Value],
    data: &[u8],
    input: &[u8],
    function: &str,
) -> NativeResult {
    clear_information(arguments);
    let Some(kind) = detect(context, data, input, function) else {
        return Ok(Value::Bool(false));
    };
    let mut extra = PhpArray::new();
    let Some(size) = headers::read(kind, data, &mut extra) else {
        return Ok(Value::Bool(false));
    };
    if let Some(info) = args::reference(arguments, 1) {
        *info.borrow_mut() = Value::from(extra);
    }
    let width = size.width;
    let height = size.height;
    let mut result = PhpArray::new();
    for number in [i64::from(width), i64::from(height), kind as i64] {
        result.push(Value::Int(number))?;
    }
    result.push(Value::from(format!(
        "width=\"{width}\" height=\"{height}\""
    )))?;
    if size.bits != 0 {
        result.insert(ArrayKey::str("bits"), Value::Int(i64::from(size.bits)));
    }
    if size.channels != 0 {
        result.insert(
            ArrayKey::str("channels"),
            Value::Int(i64::from(size.channels)),
        );
    }
    result.insert(ArrayKey::str("mime"), Value::from(kind.mime()));
    result.insert(ArrayKey::str("width_unit"), Value::from(size.width_unit));
    result.insert(ArrayKey::str("height_unit"), Value::from(size.height_unit));
    Ok(Value::from(result))
}

fn detect(context: &mut dyn Context, data: &[u8], input: &[u8], function: &str) -> Option<Kind> {
    let result = kind::detect(data);
    if result.is_none() {
        if data.len() >= 8 && data.starts_with(b"\x89PN") {
            context.warning(format!(
                "{function}(): PNG file corrupted by ASCII conversion"
            ));
        } else if data.len() < 12 {
            let prefix = input.split(|byte| *byte == 0).next().unwrap_or_default();
            let message = [
                function.as_bytes(),
                b"(): Error reading from ",
                prefix,
                b"!",
            ]
            .concat();
            // Older PHP displays this notice using the C string up to the first NUL.
            context.notice(String::from_utf8_lossy(&message).into_owned());
        }
    }
    result
}
