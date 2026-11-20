//! Shared named parameters for method and function registration.

use phera_extension_api::{ConstantValue, Param, ParamType};

use super::{CLASS, GREGORIAN};

const OBJECT: Param = Param::new("calendar", ParamType::Class(CLASS));
const GREG: Param = Param::new("calendar", ParamType::Class(GREGORIAN));
const TZ: Param = Param::new("timezone", ParamType::ObjectOrString).nullable();
const LOCALE: Param = Param::new("locale", ParamType::String)
    .nullable()
    .with_default(ConstantValue::Null);
const FIELD_VALUE: Param = Param::new("field", ParamType::Int);
const TYPE_VALUE: Param = Param::new("type", ParamType::Int);
const AMOUNT: Param = Param::new("value", ParamType::Int);
const ROLL_AMOUNT: Param = Param::new("value", ParamType::Mixed);
const STAMP_VALUE: Param = Param::new("timestamp", ParamType::Float);
const OPTIONAL_STAMP: Param = STAMP_VALUE.nullable().with_default(ConstantValue::Null);
const OPTIONAL_FIELD: Param = FIELD_VALUE.nullable().with_default(ConstantValue::Null);
const YEAR_VALUE: Param = Param::new("year", ParamType::Int);
const MONTH: Param = Param::new("month", ParamType::Int);
const DATE: Param = Param::new("dayOfMonth", ParamType::Int);
const HOUR: Param = Param::new("hour", ParamType::Int);
const MINUTE: Param = Param::new("minute", ParamType::Int);
const SECOND: Param = Param::new("second", ParamType::Int);
const OPTIONAL_SECOND: Param = SECOND.nullable().with_default(ConstantValue::Null);
const DAY_VALUE: Param = Param::new("dayOfWeek", ParamType::Int);
const DAYS_VALUE: Param = Param::new("days", ParamType::Int);
const OPTION_VALUE: Param = Param::new("option", ParamType::Int);
const LENIENT_VALUE: Param = Param::new("lenient", ParamType::Bool);
pub const INSTANCE: &[Param] = &[OBJECT];
pub const GREG_INSTANCE: &[Param] = &[GREG];
pub const CREATE: &[Param] = &[TZ.with_default(ConstantValue::Null), LOCALE];
pub const ZONE_ARG: &[Param] = &[TZ];
pub const FN_ZONE: &[Param] = &[OBJECT, TZ];
pub const FROM_DATE: &[Param] = &[Param::new("datetime", ParamType::ObjectOrString), LOCALE];
pub const KEYWORDS: &[Param] = &[
    Param::new("keyword", ParamType::String),
    Param::new("locale", ParamType::String),
    Param::new("onlyCommon", ParamType::Bool),
];
pub const FIELD: &[Param] = &[FIELD_VALUE];
pub const FN_FIELD: &[Param] = &[OBJECT, FIELD_VALUE];
pub const ALTER: &[Param] = &[FIELD_VALUE, AMOUNT];
pub const FN_ALTER: &[Param] = &[OBJECT, FIELD_VALUE, AMOUNT];
pub const ROLL: &[Param] = &[FIELD_VALUE, ROLL_AMOUNT];
pub const FN_ROLL: &[Param] = &[OBJECT, FIELD_VALUE, ROLL_AMOUNT];
pub const CLEAR: &[Param] = &[OPTIONAL_FIELD];
pub const FN_CLEAR: &[Param] = &[OBJECT, OPTIONAL_FIELD];
pub const STAMP: &[Param] = &[STAMP_VALUE];
pub const FN_STAMP: &[Param] = &[OBJECT, STAMP_VALUE];
pub const GREG_STAMP: &[Param] = &[GREG, STAMP_VALUE];
pub const WEEKEND: &[Param] = &[OPTIONAL_STAMP];
pub const FN_WEEKEND: &[Param] = &[OBJECT, OPTIONAL_STAMP];
pub const DAY: &[Param] = &[DAY_VALUE];
pub const FN_DAY: &[Param] = &[OBJECT, DAY_VALUE];
pub const DAYS: &[Param] = &[DAYS_VALUE];
pub const FN_DAYS: &[Param] = &[OBJECT, DAYS_VALUE];
pub const TYPE: &[Param] = &[TYPE_VALUE];
pub const FN_TYPE: &[Param] = &[OBJECT, TYPE_VALUE];
pub const OPTION: &[Param] = &[OPTION_VALUE];
pub const FN_OPTION: &[Param] = &[OBJECT, OPTION_VALUE];
pub const LENIENT: &[Param] = &[LENIENT_VALUE];
pub const FN_LENIENT: &[Param] = &[OBJECT, LENIENT_VALUE];
pub const OTHER: &[Param] = &[Param::new("other", ParamType::Class(CLASS))];
pub const FN_OTHER: &[Param] = &[OBJECT, OTHER[0]];
pub const DIFFERENCE: &[Param] = &[STAMP_VALUE, FIELD_VALUE];
pub const FN_DIFFERENCE: &[Param] = &[OBJECT, STAMP_VALUE, FIELD_VALUE];
pub const YEAR: &[Param] = &[YEAR_VALUE];
pub const GREG_YEAR: &[Param] = &[GREG, YEAR_VALUE];
pub const CIVIL: &[Param] = &[YEAR_VALUE, MONTH, DATE];
pub const CIVIL_FULL: &[Param] = &[YEAR_VALUE, MONTH, DATE, HOUR, MINUTE, OPTIONAL_SECOND];
pub const SET: &[Param] = &[
    YEAR_VALUE,
    MONTH,
    DATE.nullable().optional(),
    HOUR.nullable().optional(),
    MINUTE.nullable().optional(),
    SECOND.nullable().optional(),
];
pub const FN_SET: &[Param] = &[OBJECT, SET[0], SET[1], SET[2], SET[3], SET[4], SET[5]];
pub const GREG_CREATE: &[Param] = &[
    Param::new("timezoneOrYear", ParamType::Mixed).optional(),
    Param::new("localeOrMonth", ParamType::Mixed).optional(),
    Param::new("day", ParamType::Mixed).optional(),
    Param::new("hour", ParamType::Mixed).optional(),
    Param::new("minute", ParamType::Mixed).optional(),
    Param::new("second", ParamType::Mixed).optional(),
];
