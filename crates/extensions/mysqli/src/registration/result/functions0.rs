//! PHP signatures for mysqli_result.

use super::super::dispatch;
use crate::result;
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType};

pub const ENTRIES: &[FunctionEntry] = &[
    FunctionEntry::asynchronous(
        "mysqli_fetch_object",
        &[
            Param::new("result", ParamType::Class("mysqli_result")),
            Param::new("class", ParamType::String).with_default(ConstantValue::String("stdClass")),
            Param::new("constructor_args", ParamType::Array)
                .with_default(ConstantValue::EmptyArray),
        ],
        dispatch::fetch_object_proc,
    ),
    FunctionEntry::asynchronous(
        "mysqli_free_result",
        &[Param::new("result", ParamType::Class("mysqli_result"))],
        dispatch::result_async_proc::<{ result::FREE }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_fetch_row",
        &[Param::new("result", ParamType::Class("mysqli_result"))],
        dispatch::result_async_proc::<{ result::ROW }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_fetch_assoc",
        &[Param::new("result", ParamType::Class("mysqli_result"))],
        dispatch::result_async_proc::<{ result::ASSOC }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_fetch_array",
        &[
            Param::new("result", ParamType::Class("mysqli_result")),
            Param::new("mode", ParamType::Int)
                .with_constant_default("MYSQLI_BOTH", ConstantValue::Int(3)),
        ],
        dispatch::result_async_proc::<{ result::ARRAY }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_fetch_all",
        &[
            Param::new("result", ParamType::Class("mysqli_result")),
            Param::new("mode", ParamType::Int)
                .with_constant_default("MYSQLI_NUM", ConstantValue::Int(2)),
        ],
        dispatch::result_async_proc::<{ result::ALL }>,
    ),
    FunctionEntry::asynchronous(
        "mysqli_fetch_column",
        &[
            Param::new("result", ParamType::Class("mysqli_result")),
            Param::new("column", ParamType::Int).with_default(ConstantValue::Int(0)),
        ],
        dispatch::result_async_proc::<{ result::COLUMN }>,
    ),
    FunctionEntry::sync(
        "mysqli_data_seek",
        &[
            Param::new("result", ParamType::Class("mysqli_result")),
            Param::new("offset", ParamType::Int),
        ],
        dispatch::result_proc::<{ result::SEEK }>,
    ),
    FunctionEntry::sync(
        "mysqli_field_seek",
        &[
            Param::new("result", ParamType::Class("mysqli_result")),
            Param::new("index", ParamType::Int),
        ],
        dispatch::result_proc::<{ result::FIELD_SEEK }>,
    ),
    FunctionEntry::sync(
        "mysqli_field_tell",
        &[Param::new("result", ParamType::Class("mysqli_result"))],
        dispatch::result_proc::<{ result::FIELD_TELL }>,
    ),
    FunctionEntry::sync(
        "mysqli_fetch_field",
        &[Param::new("result", ParamType::Class("mysqli_result"))],
        dispatch::result_proc::<{ result::FIELD }>,
    ),
    FunctionEntry::sync(
        "mysqli_fetch_field_direct",
        &[
            Param::new("result", ParamType::Class("mysqli_result")),
            Param::new("index", ParamType::Int),
        ],
        dispatch::result_proc::<{ result::FIELD_DIRECT }>,
    ),
    FunctionEntry::sync(
        "mysqli_fetch_fields",
        &[Param::new("result", ParamType::Class("mysqli_result"))],
        dispatch::result_proc::<{ result::FIELDS }>,
    ),
    FunctionEntry::sync(
        "mysqli_num_fields",
        &[Param::new("result", ParamType::Class("mysqli_result"))],
        dispatch::result_proc::<{ result::NUM_FIELDS }>,
    ),
    FunctionEntry::sync(
        "mysqli_num_rows",
        &[Param::new("result", ParamType::Class("mysqli_result"))],
        dispatch::result_proc::<{ result::NUM_ROWS }>,
    ),
    FunctionEntry::sync(
        "mysqli_fetch_lengths",
        &[Param::new("result", ParamType::Class("mysqli_result"))],
        dispatch::result_proc::<{ result::LENGTHS }>,
    ),
];
