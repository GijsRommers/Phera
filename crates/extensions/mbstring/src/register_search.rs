//! Search and trim signatures: `mb_strpos(haystack: "abc", needle: "b")`.
use crate::{layout, search};
use phera_extension_api::{FunctionEntry, Param, ParamType, Registry};
pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "mb_strpos",
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
        |c, a| search::position(c, a, "mb_strpos", search::Search::First),
    ));
    registry.function(FunctionEntry::sync(
        "mb_strrpos",
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
        |c, a| search::position(c, a, "mb_strrpos", search::Search::Last),
    ));
    registry.function(FunctionEntry::sync(
        "mb_stripos",
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
        |c, a| search::position(c, a, "mb_stripos", search::Search::FirstFold),
    ));
    registry.function(FunctionEntry::sync(
        "mb_strripos",
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
        |c, a| search::position(c, a, "mb_strripos", search::Search::LastFold),
    ));
    registry.function(FunctionEntry::sync(
        "mb_strstr",
        const {
            &[
                Param::new("haystack", ParamType::String),
                Param::new("needle", ParamType::String),
                Param::new("before_needle", ParamType::Bool).optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        |c, a| search::portion(c, a, "mb_strstr", search::Search::First),
    ));
    registry.function(FunctionEntry::sync(
        "mb_strrchr",
        const {
            &[
                Param::new("haystack", ParamType::String),
                Param::new("needle", ParamType::String),
                Param::new("before_needle", ParamType::Bool).optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        |c, a| search::portion(c, a, "mb_strrchr", search::Search::Last),
    ));
    registry.function(FunctionEntry::sync(
        "mb_stristr",
        const {
            &[
                Param::new("haystack", ParamType::String),
                Param::new("needle", ParamType::String),
                Param::new("before_needle", ParamType::Bool).optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        |c, a| search::portion(c, a, "mb_stristr", search::Search::FirstFold),
    ));
    registry.function(FunctionEntry::sync(
        "mb_strrichr",
        const {
            &[
                Param::new("haystack", ParamType::String),
                Param::new("needle", ParamType::String),
                Param::new("before_needle", ParamType::Bool).optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        |c, a| search::portion(c, a, "mb_strrichr", search::Search::LastFold),
    ));
    registry.function(FunctionEntry::sync(
        "mb_trim",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("characters", ParamType::String)
                    .nullable()
                    .optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        |c, a| layout::trim(c, a, "mb_trim", layout::Trim::Both),
    ));
    registry.function(FunctionEntry::sync(
        "mb_ltrim",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("characters", ParamType::String)
                    .nullable()
                    .optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        |c, a| layout::trim(c, a, "mb_ltrim", layout::Trim::Left),
    ));
    registry.function(FunctionEntry::sync(
        "mb_rtrim",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("characters", ParamType::String)
                    .nullable()
                    .optional(),
                Param::new("encoding", ParamType::String)
                    .nullable()
                    .optional(),
            ]
        },
        |c, a| layout::trim(c, a, "mb_rtrim", layout::Trim::Right),
    ));
}
