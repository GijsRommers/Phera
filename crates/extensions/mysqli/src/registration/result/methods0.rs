//! PHP signatures for mysqli_result.

use super::super::dispatch;
use crate::result;
use phera_extension_api::{ConstantValue, MethodEntry, Param, ParamType};

pub const ENTRIES: &[MethodEntry] = &[
    MethodEntry::public(
        "__construct",
        &[
            Param::new("mysql", ParamType::Class("mysqli")),
            Param::new("result_mode", ParamType::Int)
                .with_constant_default("MYSQLI_STORE_RESULT", ConstantValue::Int(0)),
        ],
        result::construct,
    ),
    MethodEntry::public_async(
        "fetch_object",
        &[
            Param::new("class", ParamType::String).with_default(ConstantValue::String("stdClass")),
            Param::new("constructor_args", ParamType::Array)
                .with_default(ConstantValue::EmptyArray),
        ],
        dispatch::fetch_object,
    )
    .tentative_return("object|false|null"),
    MethodEntry::public("close", &[], dispatch::result_method::<{ result::FREE }>)
        .tentative_return("void"),
    MethodEntry::public("free", &[], dispatch::result_method::<{ result::FREE }>)
        .tentative_return("void"),
    MethodEntry::public(
        "free_result",
        &[],
        dispatch::result_method::<{ result::FREE }>,
    )
    .tentative_return("void"),
    MethodEntry::public("fetch_row", &[], dispatch::result_method::<{ result::ROW }>)
        .tentative_return("array|false|null"),
    MethodEntry::public(
        "fetch_assoc",
        &[],
        dispatch::result_method::<{ result::ASSOC }>,
    )
    .tentative_return("array|false|null"),
    MethodEntry::public(
        "fetch_array",
        &[Param::new("mode", ParamType::Int)
            .with_constant_default("MYSQLI_BOTH", ConstantValue::Int(3))],
        dispatch::result_method::<{ result::ARRAY }>,
    )
    .tentative_return("array|false|null"),
    MethodEntry::public(
        "fetch_all",
        &[Param::new("mode", ParamType::Int)
            .with_constant_default("MYSQLI_NUM", ConstantValue::Int(2))],
        dispatch::result_method::<{ result::ALL }>,
    )
    .tentative_return("array"),
    MethodEntry::public(
        "fetch_column",
        &[Param::new("column", ParamType::Int).with_default(ConstantValue::Int(0))],
        dispatch::result_method::<{ result::COLUMN }>,
    )
    .returning("string|int|float|false|null"),
    MethodEntry::public(
        "data_seek",
        &[Param::new("offset", ParamType::Int)],
        dispatch::result_method::<{ result::SEEK }>,
    )
    .tentative_return("bool"),
    MethodEntry::public(
        "field_seek",
        &[Param::new("index", ParamType::Int)],
        dispatch::result_method::<{ result::FIELD_SEEK }>,
    )
    .tentative_return("true"),
    MethodEntry::public(
        "fetch_field",
        &[],
        dispatch::result_method::<{ result::FIELD }>,
    )
    .tentative_return("object|false"),
    MethodEntry::public(
        "fetch_field_direct",
        &[Param::new("index", ParamType::Int)],
        dispatch::result_method::<{ result::FIELD_DIRECT }>,
    )
    .tentative_return("object|false"),
    MethodEntry::public(
        "fetch_fields",
        &[],
        dispatch::result_method::<{ result::FIELDS }>,
    )
    .tentative_return("array"),
    MethodEntry::public("getIterator", &[], result::get_iterator).returning("Iterator"),
];
