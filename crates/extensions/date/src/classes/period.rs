//! `new DatePeriod($start, new DateInterval('P1D'), 3)` includes four dates by default.

use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Value};
use phera_extension_api::{Param, ParamType, args, prepare_call};

use super::zone_kind::ZoneKind;
use super::{
    This,
    instant::{IMMUTABLE, INTERFACE, Instant},
    make,
};
use super::{
    interval::{Arithmetic, Interval},
    interval_iso, receiver, state, store,
};

pub const CLASS: &str = "DatePeriod";
pub const EXCLUDE_START: i64 = 1;
pub const INCLUDE_END: i64 = 2;
const MAX_RECURRENCES: i64 = i32::MAX as i64 - 8;

#[derive(Clone, Default)]
pub struct Period {
    pub start: Option<Instant>,
    pub current: Option<Instant>,
    pub end: Option<Instant>,
    pub interval: Option<Interval>,
    pub initialized: bool,
    pub recurrences: i64,
    pub include_start: bool,
    pub include_end: bool,
    pub date_class: String,
}

impl Period {
    fn finish(mut self, options: i64, site: &str) -> Result<Self, NativeError> {
        let limit = MAX_RECURRENCES + 1;
        let count = self.recurrences;
        let reason = || {
            format!(
                "{site}(): Recurrence count must be greater or equal to 1 and lower than {limit}"
            )
        };
        if self.end.is_none() && !(1..=MAX_RECURRENCES).contains(&count) {
            return Err(malformed(reason()));
        }
        self.include_start = options & EXCLUDE_START == 0;
        self.include_end = options & INCLUDE_END != 0;
        self.recurrences = count + self.extra_recurrences();
        if self.recurrences > MAX_RECURRENCES {
            return Err(NativeError::throw(
                "DateMalformedStringException",
                format!("{} (including options)", reason()),
            ));
        }
        self.initialized = true;
        Ok(self)
    }

    fn extra_recurrences(&self) -> i64 {
        i64::from(self.include_start) + i64::from(self.include_end)
    }

    pub fn new(
        start: Instant,
        interval: Interval,
        end: Option<Instant>,
        count: i64,
        class: &str,
    ) -> Self {
        Self {
            start: Some(start),
            current: None,
            end,
            interval: Some(interval),
            initialized: false,
            recurrences: count,
            include_start: true,
            include_end: false,
            date_class: class.into(),
        }
    }

    pub fn read(object: &Object) -> Result<Self, NativeError> {
        state(object, CLASS)
    }

    pub fn make(self, context: &mut dyn Context) -> NativeResult {
        make(context, CLASS, self)
    }

    pub fn store(self, object: &Object) {
        store(object, self);
    }

    pub fn properties(&self, context: &mut dyn Context) -> Result<PhpArray, NativeError> {
        let mut data = PhpArray::new();
        for field in super::period_properties::Field::ALL {
            data.insert(
                phera_extension_api::ArrayKey::str(field.name()),
                field.value(context, self)?,
            );
        }
        Ok(data)
    }

    pub(super) fn make_date(
        &self,
        context: &mut dyn Context,
        date: Option<Instant>,
    ) -> NativeResult {
        if let Some(date) = date {
            make(context, &self.date_class, date)
        } else {
            Ok(Value::Null)
        }
    }

    pub(super) fn make_interval(&self, context: &mut dyn Context) -> NativeResult {
        let Some(mut interval) = self.interval.clone() else {
            return Ok(Value::Null);
        };
        interval.arithmetic = Arithmetic::Civil;
        interval.source = None;
        interval.make(context)
    }
}

pub fn construct(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    const SITE: &str = "DatePeriod::__construct";
    let object = receiver(this)?;
    let invalid = || {
        NativeError::type_error(concat!(
            "DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), ",
            "or (DateTimeInterface, DateInterval, DateTime [, int]), ",
            "or (string [, int]) as arguments"
        ))
    };
    let start = args::object(arguments, 0).filter(|start| start.class.is_instance_of(INTERFACE));
    let interval =
        args::object(arguments, 1).filter(|interval| interval.class.is_instance_of("DateInterval"));
    if !(3..=4).contains(&arguments.len()) || start.is_none() || interval.is_none() {
        return Err(invalid());
    }
    let start = start.expect("start checked");
    let interval = interval.expect("interval checked");
    let (end, count) = if let Some(end) =
        args::object(arguments, 2).filter(|object| object.class.is_instance_of(INTERFACE))
    {
        (Some(end), 0)
    } else {
        (
            None,
            coerce_integer(context, &arguments[2], "end").map_err(|_| invalid())?,
        )
    };
    let options = arguments
        .get(3)
        .map_or(Ok(0), |value| coerce_integer(context, value, "options"))
        .map_err(|_| invalid())?;
    *object.native.borrow_mut() = None;
    let uninitialized_date = || {
        NativeError::throw(
            "DateObjectError",
            concat!(
                "Object of type DateTimeInterface has not been correctly initialized ",
                "by calling parent::__construct() in its constructor",
            ),
        )
    };
    let start_date = Instant::read(start).map_err(|_| uninitialized_date())?;
    let end = end
        .map(|date| Instant::read(date).map_err(|_| uninitialized_date()))
        .transpose()?;
    let interval = Interval::read(interval)?;
    let period =
        Period::new(start_date, interval, end, count, start.class.name()).finish(options, SITE)?;
    period.store(object);
    Ok(Value::Null)
}

fn coerce_integer(
    context: &mut dyn Context,
    value: &Value,
    name: &'static str,
) -> Result<i64, NativeError> {
    let mut argument = [value.clone()];
    prepare_call(
        "DatePeriod::__construct",
        &[Param::new(name, ParamType::Int)],
        &mut argument,
        context,
    )?;
    Ok(args::int(&argument, 0).expect("prepared integer"))
}

pub fn from_iso(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    const SITE: &str = "DatePeriod::createFromISO8601String";
    let input = args::bytes(arguments, 0);
    let options = args::int(arguments, 1).unwrap_or(0);
    let shown = String::from_utf8_lossy(input.split(|byte| *byte == 0).next().unwrap_or_default());
    let iso = interval_iso::parse_iso(input)
        .map_err(|()| malformed(format!("Unknown or bad format ({shown})")))?;
    let missing = |field| {
        malformed(format!(
            "{SITE}(): ISO interval must contain {field}, \"{shown}\" given"
        ))
    };
    let start = iso.start.ok_or_else(|| missing("a start date"))?;
    let fields = iso.fields.ok_or_else(|| missing("an interval"))?;
    let count = iso.recurrences.unwrap_or(0);
    if iso.end.is_none() && count == 0 {
        return Err(missing("an end date or a recurrence count"));
    }
    let fixed = |timestamp| Instant::new(timestamp, 0, ZoneKind::Fixed(0));
    let period = Period::new(
        fixed(start),
        Interval::new(fields, Arithmetic::Civil),
        iso.end.map(fixed),
        count,
        IMMUTABLE,
    );
    period.finish(options, SITE)?.make(context)
}

fn malformed(message: String) -> NativeError {
    NativeError::throw("DateMalformedPeriodStringException", message)
}

pub fn start(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let period = Period::read(object)?;
    if period.start.is_none() {
        return Err(super::uninitialized(object, CLASS));
    }
    period.make_date(context, period.start.clone())
}

pub fn end(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let Ok(period) = Period::read(receiver(this)?) else {
        return Ok(Value::Null);
    };
    period.make_date(context, period.end.clone())
}

pub fn interval(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let period = Period::read(object)?;
    if period.interval.is_none() {
        return Err(super::uninitialized(object, CLASS));
    }
    period.make_interval(context)
}

pub fn recurrences(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let Ok(period) = Period::read(receiver(this)?) else {
        return Ok(Value::Null);
    };
    let count = period.recurrences - period.extra_recurrences();
    Ok(if count == 0 {
        Value::Null
    } else {
        Value::Int(count)
    })
}
