//! Signatures preserve references: `preg_match('/a/', 'a', $matches)` writes `$matches`.

use crate::{callbacks, errors, grep, matching, quote, replace, split};
use phera_extension_api::{FunctionEntry, Param, ParamType, Registry};

pub(crate) fn register(registry: &mut Registry) {
    const MATCH: &[Param] = &[
        Param::new("pattern", ParamType::String),
        Param::new("subject", ParamType::String),
        Param::new("matches", ParamType::Mixed).by_ref().optional(),
        Param::new("flags", ParamType::Int).optional(),
        Param::new("offset", ParamType::Int).optional(),
    ];
    const REPLACE: &[Param] = &[
        Param::new("pattern", ParamType::ArrayOrString),
        Param::new("replacement", ParamType::ArrayOrString),
        Param::new("subject", ParamType::ArrayOrString),
        Param::new("limit", ParamType::Int).optional(),
        Param::new("count", ParamType::Mixed).by_ref().optional(),
    ];
    const CALLBACK: &[Param] = &[
        Param::new("pattern", ParamType::ArrayOrString),
        Param::new("callback", ParamType::Callable),
        Param::new("subject", ParamType::ArrayOrString),
        Param::new("limit", ParamType::Int).optional(),
        Param::new("count", ParamType::Mixed).by_ref().optional(),
        Param::new("flags", ParamType::Int).optional(),
    ];
    const CALLBACK_ARRAY: &[Param] = &[
        Param::new("pattern", ParamType::Array),
        Param::new("subject", ParamType::ArrayOrString),
        Param::new("limit", ParamType::Int).optional(),
        Param::new("count", ParamType::Mixed).by_ref().optional(),
        Param::new("flags", ParamType::Int).optional(),
    ];
    const SPLIT: &[Param] = &[
        Param::new("pattern", ParamType::String),
        Param::new("subject", ParamType::String),
        Param::new("limit", ParamType::Int).optional(),
        Param::new("flags", ParamType::Int).optional(),
    ];
    const QUOTE: &[Param] = &[
        Param::new("str", ParamType::String),
        Param::new("delimiter", ParamType::String)
            .nullable()
            .optional(),
    ];
    const GREP: &[Param] = &[
        Param::new("pattern", ParamType::String),
        Param::new("array", ParamType::Array),
        Param::new("flags", ParamType::Int).optional(),
    ];
    registry.function(FunctionEntry::sync("preg_match", MATCH, matching::single));
    registry.function(FunctionEntry::sync("preg_match_all", MATCH, matching::all));
    registry.function(FunctionEntry::sync(
        "preg_replace",
        REPLACE,
        replace::replace,
    ));
    registry.function(FunctionEntry::sync("preg_filter", REPLACE, replace::filter));
    registry.function(FunctionEntry::asynchronous(
        "preg_replace_callback",
        CALLBACK,
        callbacks::replace,
    ));
    registry.function(FunctionEntry::asynchronous(
        "preg_replace_callback_array",
        CALLBACK_ARRAY,
        callbacks::replace_array,
    ));
    registry.function(FunctionEntry::sync("preg_split", SPLIT, split::split));
    registry.function(FunctionEntry::sync("preg_quote", QUOTE, quote::quote));
    registry.function(FunctionEntry::sync("preg_grep", GREP, grep::grep));
    registry.function(FunctionEntry::sync("preg_last_error", &[], errors::last));
    registry.function(FunctionEntry::sync(
        "preg_last_error_msg",
        &[],
        errors::message,
    ));
}
