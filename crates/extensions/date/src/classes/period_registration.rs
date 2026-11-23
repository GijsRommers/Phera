//! `DatePeriod::INCLUDE_END_DATE` includes an end instant on the interval boundary.

use phera_extension_api::{ClassEntry, ConstantValue, MethodEntry, Param, ParamType};
use phera_extension_api::{PropertyEntry, PropertyType, Registry, Visibility};

use super::{period, period_serialization as serialization};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["IteratorAggregate"],
        constants: &[
            (
                "EXCLUDE_START_DATE",
                ConstantValue::Int(period::EXCLUDE_START),
            ),
            ("INCLUDE_END_DATE", ConstantValue::Int(period::INCLUDE_END)),
        ],
        properties: PROPERTIES,
        hooks: Some(&super::object_hooks::PERIOD),
        methods: METHODS,
        ..ClassEntry::class(period::CLASS)
    });
}

const PROPERTIES: &[PropertyEntry] = &[
    property("start", PropertyType::NullableClass("DateTimeInterface")),
    property("current", PropertyType::NullableClass("DateTimeInterface")),
    property("end", PropertyType::NullableClass("DateTimeInterface")),
    property("interval", PropertyType::NullableClass("DateInterval")),
    property("recurrences", PropertyType::Int),
    property("include_start_date", PropertyType::Bool),
    property("include_end_date", PropertyType::Bool),
];

const METHODS: &[MethodEntry] = &[
    MethodEntry::public(
        "__construct",
        &[
            Param::new("start", ParamType::Mixed).arity_checked_by_handler(),
            Param::new("interval", ParamType::Mixed).optional(),
            Param::new("end", ParamType::Mixed).optional(),
            Param::new("options", ParamType::Mixed).optional(),
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
    MethodEntry::public("getIterator", &[], super::period_dates::get_iterator)
        .returning("Iterator"),
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

const fn property(name: &'static str, ty: PropertyType) -> PropertyEntry {
    PropertyEntry::typed(name, ty, Visibility::Public).readonly()
}
