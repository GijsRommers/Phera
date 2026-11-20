//! Decode and encode images, for example `imagepng(imagecreatefrompng('photo.png'))`.

mod formats;
mod gif;
mod png;
mod raster;

use phera_ext_streams::{
    file_io::blocking,
    native::{StreamContents, inspect_path, write_path, write_resource},
};
use phera_extension_api::{
    ArrayKey, ConstantValue as C, Context, FunctionEntry as F, LocalFuture, NativeError,
    NativeResult, Param as P, ParamType as T, PhpArray, Registry, Value, args,
};

use crate::core::{IMAGE, with, wrap};
use formats::{Format, Options};

const FILENAME: P = P::new("filename", T::String).without_nul();
const DATA: P = P::new("data", T::String);
const FILE: P = P::new("file", T::Untyped).with_default(C::Null);
const QUALITY: P = P::new("quality", T::Int).with_default(C::Int(-1));
const FILTERS: P = P::new("filters", T::Int).with_default(C::Int(-1));
const COMPRESSED: P = P::new("compressed", T::Bool).with_default(C::Bool(true));
const LIMIT: u64 = 64 * 1024 * 1024;

pub fn register(registry: &mut Registry) {
    for entry in const {
        [
            F::asynchronous("imagecreatefromstring", &[DATA], from_string),
            F::asynchronous("imagecreatefrompng", &[FILENAME], |cx, args| {
                Box::pin(from_file(cx, args, Format::Png))
            }),
            F::asynchronous("imagecreatefromgif", &[FILENAME], |cx, args| {
                Box::pin(from_file(cx, args, Format::Gif))
            }),
            F::asynchronous("imagecreatefromjpeg", &[FILENAME], |cx, args| {
                Box::pin(from_file(cx, args, Format::Jpeg))
            }),
            F::asynchronous("imagecreatefrombmp", &[FILENAME], |cx, args| {
                Box::pin(from_file(cx, args, Format::Bmp))
            }),
            F::asynchronous("imagecreatefromwebp", &[FILENAME], |cx, args| {
                Box::pin(from_file(cx, args, Format::Webp))
            }),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, "GdImage|false", None);
    }
    for entry in const {
        [
            F::asynchronous("imagepng", &[IMAGE, FILE, QUALITY, FILTERS], |cx, args| {
                Box::pin(output(cx, args, Format::Png))
            }),
            F::asynchronous("imagegif", &[IMAGE, FILE], |cx, args| {
                Box::pin(output(cx, args, Format::Gif))
            }),
            F::asynchronous("imagejpeg", &[IMAGE, FILE, QUALITY], |cx, args| {
                Box::pin(output(cx, args, Format::Jpeg))
            }),
            F::asynchronous("imagebmp", &[IMAGE, FILE, COMPRESSED], |cx, args| {
                Box::pin(output(cx, args, Format::Bmp))
            }),
            F::asynchronous("imagewebp", &[IMAGE, FILE, QUALITY], |cx, args| {
                Box::pin(output(cx, args, Format::Webp))
            }),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, "bool", None);
    }
    registry.function(F::sync("gd_info", &[], gd_info));
    registry.describe_function("gd_info", "array", None);
    registry.function(F::sync("imagetypes", &[], |_, _| {
        Ok(Value::Int(1 | 2 | 4 | 32 | 64))
    }));
    registry.describe_function("imagetypes", "int", None);
}

fn gd_info(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut gd = PhpArray::new();
    gd.insert(ArrayKey::str("GD Version"), Value::from("2.3.3"));
    for (name, supported) in [
        ("FreeType Support", false),
        ("GIF Read Support", true),
        ("GIF Create Support", true),
        ("JPEG Support", true),
        ("PNG Support", true),
        ("WBMP Support", false),
        ("XPM Support", false),
        ("XBM Support", false),
        ("WebP Support", true),
        ("BMP Support", true),
        ("AVIF Support", false),
        ("TGA Read Support", false),
        ("JIS-mapped Japanese Font Support", false),
    ] {
        gd.insert(ArrayKey::str(name), Value::Bool(supported));
    }
    Ok(Value::from(gd))
}

fn from_string(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let bytes = args::bytes(&arguments, 0);
        let Some(format) = Format::detect(bytes) else {
            context.warning("imagecreatefromstring(): Data is not in a recognized format".into());
            return Ok(Value::Bool(false));
        };
        if bytes.len() as u64 > LIMIT {
            context
                .warning("imagecreatefromstring(): Encoded image exceeds the input limit".into());
            return Ok(Value::Bool(false));
        }
        let bytes = bytes.to_vec();
        match blocking(move || format.decode(&bytes)).await? {
            Ok(image) => wrap(context, image),
            Err(error) => {
                if error.contains("limit") {
                    context.warning(format!("imagecreatefromstring(): {error}"));
                }
                context.warning(format!(
                    "imagecreatefromstring(): Passed data is not in \"{}\" format",
                    format.label()
                ));
                context.warning(
                    "imagecreatefromstring(): Couldn't create GD Image Stream out of Data".into(),
                );
                Ok(Value::Bool(false))
            }
        }
    })
}

async fn from_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    format: Format,
) -> NativeResult {
    let path = args::bytes(&arguments, 0);
    let name = format.reader();
    if path.is_empty() {
        return Err(NativeError::value_error("Path must not be empty"));
    }
    let contents = inspect_path(context, name, path, LIMIT + 1, 0).await?;
    let result = match contents {
        Some(StreamContents::Bytes { prefix, .. }) if prefix.len() as u64 <= LIMIT => {
            blocking(move || format.decode(&prefix)).await?
        }
        Some(StreamContents::Bytes { .. }) => Err("Encoded image exceeds the input limit".into()),
        Some(StreamContents::Directory) => Err("Is a directory".into()),
        None => return Ok(Value::Bool(false)),
    };
    match result {
        Ok(image) => wrap(context, image),
        Err(error) => {
            let uri = String::from_utf8_lossy(path);
            context.warning(format!("{name}(): {error}"));
            context.warning(format!(
                "{name}(): \"{uri}\" is not a valid {} file",
                format.label()
            ));
            Ok(Value::Bool(false))
        }
    }
}

async fn output(context: &mut dyn Context, arguments: Vec<Value>, format: Format) -> NativeResult {
    let function = format.writer();
    let destination = args::value(&arguments, 1);
    validate_destination(destination, function)?;
    let options = Options::from_arguments(&arguments, format)?;
    let image = with(&arguments, 0, |image| Ok(image.clone()))?;
    let data = match blocking(move || format.encode(&image, options)).await? {
        Ok(data) => data,
        Err(error) => {
            context.warning(format!("{function}(): {error}"));
            return Ok(Value::Bool(false));
        }
    };
    match destination {
        Value::Null => {
            context.write(&data);
            Ok(Value::Bool(true))
        }
        Value::String(path) => write_path(context, function, path.as_bytes(), data).await,
        Value::Resource(resource) => {
            let result = write_resource(context, resource, &data, function).await?;
            Ok(Value::Bool(
                matches!(result, Value::Int(count) if count as usize == data.len()),
            ))
        }
        _ => Ok(Value::Bool(false)),
    }
}

fn validate_destination(destination: &Value, function: &str) -> Result<(), NativeError> {
    let message = match destination {
        Value::String(path) if path.as_bytes().contains(&0) => {
            "Argument #2 ($file) must not contain null bytes".into()
        }
        Value::String(path) if path.is_empty() => {
            return Err(NativeError::value_error("Path must not be empty"));
        }
        Value::String(_) | Value::Null => return Ok(()),
        Value::Resource(resource) if resource.kind == "stream" && !resource.is_closed() => {
            return Ok(());
        }
        Value::Resource(_) => "supplied resource is not a valid stream resource".into(),
        other => format!(
            "Argument #2 ($file) must be a file name or a stream resource, {} given",
            other.type_name()
        ),
    };
    Err(NativeError::type_error(format!("{function}(): {message}")))
}
