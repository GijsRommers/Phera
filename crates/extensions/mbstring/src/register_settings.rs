//! Named settings match PHP: `mb_internal_encoding(encoding: "UTF-8") === true`.
use crate::{conversion, settings};
use phera_extension_api::{FunctionEntry, Param, ParamType, Registry};
pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "mb_list_encodings",
        const { &[] },
        settings::encodings,
    ));
    registry.function(FunctionEntry::sync(
        "mb_encoding_aliases",
        const { &[Param::new("encoding", ParamType::String)] },
        settings::aliases,
    ));
    registry.function(FunctionEntry::sync(
        "mb_preferred_mime_name",
        const { &[Param::new("encoding", ParamType::String)] },
        settings::mime,
    ));
    registry.function(FunctionEntry::sync(
        "mb_substitute_character",
        const {
            &[Param::new("substitute_character", ParamType::IntOrString)
                .nullable()
                .optional()]
        },
        settings::substitute,
    ));
    registry.function(FunctionEntry::sync(
        "mb_language",
        const {
            &[Param::new("language", ParamType::String)
                .nullable()
                .optional()]
        },
        settings::language,
    ));
    registry.function(FunctionEntry::sync(
        "mb_http_input",
        const { &[Param::new("type", ParamType::String).nullable().optional()] },
        settings::input,
    ));
    registry.function(FunctionEntry::sync(
        "mb_detect_order",
        const {
            &[Param::new("encoding", ParamType::ArrayOrString)
                .nullable()
                .optional()]
        },
        conversion::order,
    ));
    registry.function(FunctionEntry::sync(
        "mb_convert_encoding",
        const {
            &[
                Param::new("string", ParamType::ArrayOrString),
                Param::new("to_encoding", ParamType::String),
                Param::new("from_encoding", ParamType::ArrayOrString)
                    .nullable()
                    .optional(),
            ]
        },
        conversion::convert,
    ));
    registry.function(FunctionEntry::sync(
        "mb_check_encoding",
        const {
            &[
                Param::new("value", ParamType::ArrayOrString)
                    .nullable()
                    .optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        conversion::check,
    ));
    registry.function(FunctionEntry::sync(
        "mb_detect_encoding",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("encodings", ParamType::ArrayOrString)
                    .nullable()
                    .optional(),
                Param::new("strict", ParamType::Bool).optional(),
            ]
        },
        conversion::detect,
    ));
    registry.function(FunctionEntry::sync(
        "mb_internal_encoding",
        const {
            &[Param::new("encoding", ParamType::String)
                .nullable()
                .optional()]
        },
        settings::internal_encoding,
    ));
    registry.function(FunctionEntry::sync(
        "mb_http_output",
        const {
            &[Param::new("encoding", ParamType::String)
                .nullable()
                .optional()]
        },
        settings::http_output,
    ));
    registry.function(FunctionEntry::sync(
        "mb_regex_encoding",
        const {
            &[Param::new("encoding", ParamType::String)
                .nullable()
                .optional()]
        },
        settings::regex_encoding,
    ));
}
