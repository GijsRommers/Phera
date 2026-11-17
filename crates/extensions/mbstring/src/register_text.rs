//! Native signatures match PHP: named arguments retain their declared names.
use crate::{layout, search, text};
use phera_extension_api::{FunctionEntry, Param, ParamType, Registry};
pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "mb_strlen",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::strlen,
    ));
    registry.function(FunctionEntry::sync(
        "mb_substr",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("start", ParamType::Int),
                Param::new("length", ParamType::Int).nullable().optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::substr,
    ));
    registry.function(FunctionEntry::sync(
        "mb_str_split",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("length", ParamType::Int).optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::split,
    ));
    registry.function(FunctionEntry::sync(
        "mb_strtoupper",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::upper,
    ));
    registry.function(FunctionEntry::sync(
        "mb_strtolower",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::lower,
    ));
    registry.function(FunctionEntry::sync(
        "mb_ucfirst",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::ucfirst,
    ));
    registry.function(FunctionEntry::sync(
        "mb_lcfirst",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::lcfirst,
    ));
    registry.function(FunctionEntry::sync(
        "mb_convert_case",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("mode", ParamType::Int),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::convert_case,
    ));
    registry.function(FunctionEntry::sync(
        "mb_ord",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::ord,
    ));
    registry.function(FunctionEntry::sync(
        "mb_chr",
        const {
            &[
                Param::new("codepoint", ParamType::Int),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::chr,
    ));
    registry.function(FunctionEntry::sync(
        "mb_scrub",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::scrub,
    ));
    registry.function(FunctionEntry::sync(
        "mb_strwidth",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        text::width,
    ));
    registry.function(FunctionEntry::sync(
        "mb_str_pad",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("length", ParamType::Int),
                Param::new("pad_string", ParamType::String).optional(),
                Param::new("pad_type", ParamType::Int).optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        layout::pad,
    ));
    registry.function(FunctionEntry::sync(
        "mb_strcut",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("start", ParamType::Int),
                Param::new("length", ParamType::Int).nullable().optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        layout::cut,
    ));
    registry.function(FunctionEntry::sync(
        "mb_strimwidth",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("start", ParamType::Int),
                Param::new("width", ParamType::Int),
                Param::new("trim_marker", ParamType::String).optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        layout::imwidth,
    ));
    registry.function(FunctionEntry::sync(
        "mb_substr_count",
        const {
            &[
                Param::new("haystack", ParamType::String),
                Param::new("needle", ParamType::String),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        search::count,
    ));
}
