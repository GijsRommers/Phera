//! `DatePeriod::INCLUDE_END_DATE` includes an end instant on the interval boundary.

use phera_extension_api::{ClassEntry, ConstantValue, MethodEntry, Param, ParamType};
use phera_extension_api::{PropertyEntry, Registry, Visibility};

use super::{period, period_properties, period_serialization as serialization};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        constants: &[
            (
                "EXCLUDE_START_DATE",
                ConstantValue::Int(period::EXCLUDE_START),
            ),
            ("INCLUDE_END_DATE", ConstantValue::Int(period::INCLUDE_END)),
        ],
        properties: PROPERTIES,
        property_access: Some(&period_properties::ACCESS),
        methods: METHODS,
        ..ClassEntry::class(period::CLASS)
    });
}

const PROPERTIES: &[PropertyEntry] = &[
    property("start", ConstantValue::Null),
    property("current", ConstantValue::Null),
    property("end", ConstantValue::Null),
    property("interval", ConstantValue::Null),
    property("recurrences", ConstantValue::Int(0)),
    property("include_start_date", ConstantValue::Bool(false)),
    property("include_end_date", ConstantValue::Bool(false)),
];

const METHODS: &[MethodEntry] = &[
    MethodEntry::public(
        "__construct",
        &[
            Param::new("start", ParamType::Mixed).optional(),
            Param::new("interval", ParamType::Mixed).optional(),
            Param::new("end", ParamType::Mixed).optional(),
            Param::new("options", ParamType::Mixed).variadic(),
        ],
        period::construct,
    ),
    MethodEntry::public(
        "createFromISO8601String",
        &[
            Param::new("specification", ParamType::String),
            Param::new("options", ParamType::Int).optional(),
        ],
        period::from_iso,
    )
    .static_(),
    MethodEntry::public("getStartDate", &[], period::start),
    MethodEntry::public("getEndDate", &[], period::end),
    MethodEntry::public("getDateInterval", &[], period::interval),
    MethodEntry::public("getRecurrences", &[], period::recurrences),
    MethodEntry::public("__serialize", &[], serialization::serialize),
    MethodEntry::public(
        "__unserialize",
        &[Param::new("data", ParamType::Array)],
        serialization::unserialize,
    ),
    MethodEntry::public(
        "__set_state",
        &[Param::new("array", ParamType::Array)],
        serialization::set_state,
    )
    .static_(),
];

const fn property(name: &'static str, value: ConstantValue) -> PropertyEntry {
    PropertyEntry::new(name, value, Visibility::Public).readonly()
}
