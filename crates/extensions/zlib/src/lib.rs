//! PHP zlib compression, streaming contexts, and gzip files.

mod contexts;
mod deflate;
mod format;
mod inflate;
mod oneshot;
mod options;
mod output;

use phera_extension_api::{ConstantValue, Extension, FunctionEntry, Param, ParamType, Registry};

use format::Encoding;

pub struct Zlib;

const DATA: Param = Param::new("data", ParamType::String);
const LEVEL: Param = Param::new("level", ParamType::Int).with_default(ConstantValue::Int(-1));
const ENCODING: Param = Param::new("encoding", ParamType::Int);
const LIMIT: Param = Param::new("max_length", ParamType::Int).with_default(ConstantValue::Int(0));

const GZIP: Param = ENCODING.with_constant_default("ZLIB_ENCODING_GZIP", ConstantValue::Int(31));
const ZLIB: Param = ENCODING.with_constant_default("ZLIB_ENCODING_DEFLATE", ConstantValue::Int(15));
const RAW: Param = ENCODING.with_constant_default("ZLIB_ENCODING_RAW", ConstantValue::Int(-15));

const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync("gzcompress", &[DATA, LEVEL, ZLIB], |context, values| {
        oneshot::encode(context, values, "gzcompress", Some(Encoding::Zlib))
    }),
    FunctionEntry::sync("gzdeflate", &[DATA, LEVEL, RAW], |context, values| {
        oneshot::encode(context, values, "gzdeflate", Some(Encoding::Raw))
    }),
    FunctionEntry::sync("gzencode", &[DATA, LEVEL, GZIP], |context, values| {
        oneshot::encode(context, values, "gzencode", Some(Encoding::Gzip))
    }),
    FunctionEntry::sync(
        "zlib_encode",
        &[DATA, ENCODING, LEVEL],
        |context, values| oneshot::encode(context, values, "zlib_encode", None),
    ),
    FunctionEntry::sync("gzuncompress", &[DATA, LIMIT], |context, values| {
        oneshot::decode(context, values, "gzuncompress", Some(Encoding::Zlib))
    }),
    FunctionEntry::sync("gzinflate", &[DATA, LIMIT], |context, values| {
        oneshot::decode(context, values, "gzinflate", Some(Encoding::Raw))
    }),
    FunctionEntry::sync("gzdecode", &[DATA, LIMIT], |context, values| {
        oneshot::decode(context, values, "gzdecode", Some(Encoding::Gzip))
    }),
    FunctionEntry::sync("zlib_decode", &[DATA, LIMIT], |context, values| {
        oneshot::decode(context, values, "zlib_decode", None)
    }),
    FunctionEntry::sync("zlib_get_coding_type", &[], output::coding_type),
    FunctionEntry::sync(
        "ob_gzhandler",
        &[DATA, Param::new("flags", ParamType::Int)],
        output::gzhandler,
    ),
];

impl Extension for Zlib {
    fn name(&self) -> &'static str {
        "zlib"
    }

    fn register(&self, registry: &mut Registry) {
        for (name, value) in [
            ("FORCE_GZIP", Encoding::Gzip.php()),
            ("FORCE_DEFLATE", Encoding::Zlib.php()),
            ("ZLIB_ENCODING_RAW", Encoding::Raw.php()),
            ("ZLIB_ENCODING_GZIP", Encoding::Gzip.php()),
            ("ZLIB_ENCODING_DEFLATE", Encoding::Zlib.php()),
            ("ZLIB_NO_FLUSH", 0),
            ("ZLIB_PARTIAL_FLUSH", 1),
            ("ZLIB_SYNC_FLUSH", 2),
            ("ZLIB_FULL_FLUSH", 3),
            ("ZLIB_FINISH", 4),
            ("ZLIB_BLOCK", 5),
            ("ZLIB_DEFAULT_STRATEGY", 0),
            ("ZLIB_FILTERED", 1),
            ("ZLIB_HUFFMAN_ONLY", 2),
            ("ZLIB_RLE", 3),
            ("ZLIB_FIXED", 4),
            ("ZLIB_VERNUM", 0x12c0),
            ("ZLIB_OK", 0),
            ("ZLIB_STREAM_END", 1),
            ("ZLIB_NEED_DICT", 2),
            ("ZLIB_ERRNO", -1),
            ("ZLIB_STREAM_ERROR", -2),
            ("ZLIB_DATA_ERROR", -3),
            ("ZLIB_MEM_ERROR", -4),
            ("ZLIB_BUF_ERROR", -5),
            ("ZLIB_VERSION_ERROR", -6),
        ] {
            registry.constant(name, value);
        }
        registry.constant("ZLIB_VERSION", ConstantValue::String("1.2.12"));
        for function in FUNCTIONS {
            registry.function(*function);
            registry.describe_function(function.name, "string|false", None);
        }
        for (function, result) in contexts::FUNCTIONS {
            registry.function(*function);
            registry.describe_function(function.name, result, None);
        }
        for class in contexts::CLASSES {
            registry.class(class);
        }
        phera_ext_streams::register_gzip_functions(registry);
    }
}
