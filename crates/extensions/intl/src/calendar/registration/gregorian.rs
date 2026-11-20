//! Gregorian-specific constructors and cutover methods.

use super::{Factory, MethodEntry, Op, create, invoke, parameters::*};

pub(super) const GREG_METHODS: &[MethodEntry] = &[
    MethodEntry::public("__construct", GREG_CREATE, |cx, object, values| {
        create(
            cx,
            object,
            values,
            "IntlGregorianCalendar::__construct",
            Factory::Gregorian,
        )
    }),
    MethodEntry::public("createFromDate", CIVIL, |cx, _, values| {
        create(
            cx,
            None,
            values,
            "IntlGregorianCalendar::createFromDate",
            Factory::Date,
        )
    })
    .static_()
    .returning("static"),
    MethodEntry::public("createFromDateTime", CIVIL_FULL, |cx, _, values| {
        create(
            cx,
            None,
            values,
            "IntlGregorianCalendar::createFromDateTime",
            Factory::DateTime,
        )
    })
    .static_()
    .returning("static"),
    MethodEntry::public("getGregorianChange", &[], |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlGregorianCalendar::getGregorianChange",
            Op::GregorianChange,
        )
    })
    .returning("float"),
    MethodEntry::public("setGregorianChange", STAMP, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlGregorianCalendar::setGregorianChange",
            Op::SetGregorianChange,
        )
    })
    .returning("bool"),
    MethodEntry::public("isLeapYear", YEAR, |cx, object, values| {
        invoke(
            cx,
            object,
            values,
            "IntlGregorianCalendar::isLeapYear",
            Op::Leap,
        )
    })
    .returning("bool"),
];
