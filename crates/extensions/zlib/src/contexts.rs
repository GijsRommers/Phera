//! Native zlib contexts retain codec state across additions and cannot be cloned.

use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, FunctionEntry, MethodEntry,
    NativeClone, NativeError, NativeResult, Object, Param, ParamType, PhpArray, Value, args,
};

use crate::deflate::{Encoder, Flush};
use crate::format::Encoding;
use crate::inflate::{Decoder, Failure};
use crate::oneshot::{ENCODINGS, argument};
use crate::options;

const DEFLATE: &str = "DeflateContext";
const INFLATE: &str = "InflateContext";
const INIT: &[Param] = &[
    Param::new("encoding", ParamType::Int),
    Param::new("options", ParamType::ObjectOrArray).optional(),
];
const DEFLATE_CONTEXT: Param = Param::new("context", ParamType::Class(DEFLATE));
const INFLATE_CONTEXT: Param = Param::new("context", ParamType::Class(INFLATE));
const DATA: Param = Param::new("data", ParamType::String);
const FLUSH: Param = Param::new("flush_mode", ParamType::Int)
    .with_constant_default("ZLIB_SYNC_FLUSH", ConstantValue::Int(2));

pub(crate) const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync("deflate_init", INIT, deflate_init),
        "DeflateContext|false",
    ),
    (
        FunctionEntry::sync("deflate_add", &[DEFLATE_CONTEXT, DATA, FLUSH], deflate_add),
        "string|false",
    ),
    (
        FunctionEntry::sync("inflate_init", INIT, inflate_init),
        "InflateContext|false",
    ),
    (
        FunctionEntry::sync("inflate_add", &[INFLATE_CONTEXT, DATA, FLUSH], inflate_add),
        "string|false",
    ),
    (
        FunctionEntry::sync("inflate_get_status", &[INFLATE_CONTEXT], status),
        "int",
    ),
    (
        FunctionEntry::sync("inflate_get_read_len", &[INFLATE_CONTEXT], read_len),
        "int",
    ),
];

const CONTEXT_HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    fork: Some(phera_extension_api::fork_refused_handle),
    forbid_serialization: true,
    ..ClassHooks::EMPTY
};

pub(crate) const CLASSES: [ClassEntry; 2] = [
    ClassEntry {
        kind: ClassKind::FinalClass,
        methods: &[MethodEntry::public("__construct", &[], reject_deflate)],
        hooks: Some(&CONTEXT_HOOKS),
        ..ClassEntry::class(DEFLATE)
    },
    ClassEntry {
        kind: ClassKind::FinalClass,
        methods: &[MethodEntry::public("__construct", &[], reject_inflate)],
        hooks: Some(&CONTEXT_HOOKS),
        ..ClassEntry::class(INFLATE)
    },
];

fn reject_deflate(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "Cannot directly construct DeflateContext, use deflate_init() instead",
    ))
}

fn reject_inflate(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "Cannot directly construct InflateContext, use inflate_init() instead",
    ))
}

fn allocate(context: &mut dyn Context, class: &str, state: impl std::any::Any) -> NativeResult {
    let value = context.create_object(class, PhpArray::new())?;
    if let Value::Object(object) = &value {
        *object.native.borrow_mut() = Some(Box::new(state));
    }
    Ok(value)
}

fn deflate_init(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let function = "deflate_init";
    let options = options::properties(args::value(values, 1));
    let level = options::integer(context, &options, "level", -1)?;
    if !(-1..=9).contains(&level) {
        return Err(NativeError::value_error(
            "deflate_init(): \"level\" option must be between -1 and 9",
        ));
    }
    let memory = options::integer(context, &options, "memory", 8)?;
    if !(1..=9).contains(&memory) {
        return Err(NativeError::value_error(
            "deflate_init(): \"memory\" option must be between 1 and 9",
        ));
    }
    let window = options::integer(context, &options, "window", 15)?;
    if !(8..=15).contains(&window) {
        return Err(NativeError::value_error(
            "deflate_init(): \"window\" option must be between 8 and 15",
        ));
    }
    let strategy = options::integer(context, &options, "strategy", 0)?;
    if !(0..=4).contains(&strategy) {
        return Err(NativeError::value_error(concat!(
            "deflate_init(): \"strategy\" option must be one of ZLIB_FILTERED, ",
            "ZLIB_HUFFMAN_ONLY, ZLIB_RLE, ZLIB_FIXED, or ZLIB_DEFAULT_STRATEGY",
        )));
    }
    let encoding = Encoding::from_php(args::int(values, 0).unwrap_or(0))
        .ok_or_else(|| argument(function, 1, "encoding", ENCODINGS))?;
    let dictionary = options::dictionary(context, &options, function)?;
    if window == 8 && encoding != Encoding::Zlib {
        context.warning("deflate_init(): Failed allocating zlib.deflate context".into());
        return Ok(Value::Bool(false));
    }
    let level = if level == -1 { 6 } else { level as u32 };
    allocate(
        context,
        DEFLATE,
        Encoder::new(encoding, level, window as u8, dictionary),
    )
}

fn inflate_init(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let options = options::properties(args::value(values, 1));
    let window = options::integer(context, &options, "window", 15)?;
    if !(8..=15).contains(&window) {
        return Err(NativeError::value_error(format!(
            "zlib window size (logarithm) ({window}) must be within 8..15",
        )));
    }
    let encoding = Encoding::from_php(args::int(values, 0).unwrap_or(0)).ok_or_else(|| {
        NativeError::value_error(
            "Encoding mode must be ZLIB_ENCODING_RAW, ZLIB_ENCODING_GZIP or ZLIB_ENCODING_DEFLATE",
        )
    })?;
    let dictionary = options::dictionary(context, &options, "inflate_init")?;
    allocate(
        context,
        INFLATE,
        Decoder::new(encoding, window as u8, dictionary),
    )
}

fn flush(values: &[Value], function: &str) -> Result<Flush, NativeError> {
    Flush::from_php(args::int(values, 2).unwrap_or(2)).ok_or_else(|| {
        argument(
            function,
            3,
            "flush_mode",
            concat!(
                "must be one of ZLIB_NO_FLUSH, ZLIB_PARTIAL_FLUSH, ZLIB_SYNC_FLUSH, ",
                "ZLIB_FULL_FLUSH, ZLIB_BLOCK, or ZLIB_FINISH"
            ),
        )
    })
}

fn with_context<T: 'static, R>(
    values: &[Value],
    function: &str,
    f: impl FnOnce(&mut T) -> R,
) -> Result<R, NativeError> {
    let invalid = || NativeError::error(format!("{function}(): invalid compression context"));
    let value = args::object(values, 0).ok_or_else(invalid)?;
    let mut state = value.native.borrow_mut();
    let state = state
        .as_mut()
        .and_then(|state| state.downcast_mut::<T>())
        .ok_or_else(invalid)?;
    Ok(f(state))
}

fn deflate_add(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let flush = flush(values, "deflate_add")?;
    let compressed = with_context(values, "deflate_add", |state: &mut Encoder| {
        state.add(args::bytes(values, 1), flush)
    })?;
    match compressed {
        Ok(output) => Ok(Value::from(output)),
        Err(()) => {
            context.warning("deflate_add(): zlib error (stream error)".into());
            Ok(Value::Bool(false))
        }
    }
}

fn inflate_add(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let flush = flush(values, "inflate_add")?;
    let data = with_context(values, "inflate_add", |state: &mut Decoder| {
        state.add(args::bytes(values, 1), flush)
    })?;
    match data {
        Ok(output) => Ok(Value::from(output)),
        Err(error) => {
            let message = match error {
                Failure::NeedDictionary => concat!(
                    "Inflating this data requires a preset dictionary, ",
                    "please specify it in the options array of inflate_init()",
                ),
                Failure::WrongDictionary => {
                    "Dictionary does not match expected dictionary (incorrect adler32 hash)"
                }
                _ => error.message(),
            };
            context.warning(format!("inflate_add(): {message}"));
            Ok(Value::Bool(false))
        }
    }
}

fn status(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    with_context(values, "inflate_get_status", |state: &mut Decoder| {
        Value::Int(state.status)
    })
}

fn read_len(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    with_context(values, "inflate_get_read_len", |state: &mut Decoder| {
        Value::Int(state.read_len as i64)
    })
}
