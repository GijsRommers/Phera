//! MIME signatures match PHP: `iconv_mime_encode(field_name: "Subject", field_value: "hi")`.
use crate::{iconv, iconv_mime};
use phera_extension_api::{FunctionEntry, Param, ParamType, Registry};
pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "iconv",
        const {
            &[
                Param::new("from_encoding", ParamType::String),
                Param::new("to_encoding", ParamType::String),
                Param::new("string", ParamType::String),
            ]
        },
        iconv::convert,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_strlen",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        iconv::strlen,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_substr",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("offset", ParamType::Int),
                Param::new("length", ParamType::Int).nullable().optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        iconv::substr,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_strpos",
        const {
            &[
                Param::new("haystack", ParamType::String),
                Param::new("needle", ParamType::String),
                Param::new("offset", ParamType::Int).optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        |c, a| iconv::position(c, a, false),
    ));
    registry.function(FunctionEntry::sync(
        "iconv_strrpos",
        const {
            &[
                Param::new("haystack", ParamType::String),
                Param::new("needle", ParamType::String),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        |c, a| iconv::position(c, a, true),
    ));
    registry.function(FunctionEntry::sync(
        "iconv_get_encoding",
        const { &[Param::new("type", ParamType::String).optional()] },
        iconv::get,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_set_encoding",
        const {
            &[
                Param::new("type", ParamType::String),
                Param::new("encoding", ParamType::String),
            ]
        },
        iconv::set,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_mime_encode",
        const {
            &[
                Param::new("field_name", ParamType::String),
                Param::new("field_value", ParamType::String),
                Param::new("options", ParamType::Array).optional(),
            ]
        },
        iconv_mime::encode,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_mime_decode",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("mode", ParamType::Int).optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        iconv_mime::decode,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_mime_decode_headers",
        const {
            &[
                Param::new("headers", ParamType::String),
                Param::new("mode", ParamType::Int).optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        iconv_mime::headers,
    ));
}
