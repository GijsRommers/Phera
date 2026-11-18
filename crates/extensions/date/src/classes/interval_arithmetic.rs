//! `PT24H` adds 24 elapsed hours, while `P1D` advances one calendar day.

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use super::{
    This,
    instant::Instant,
    interval::{Arithmetic, Interval},
    interval_diff, receiver,
};
use crate::parse::state::State;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Add,
    Subtract,
}

pub fn add(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    change(context, this, arguments, Direction::Add, false)
}

pub fn sub(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    change(context, this, arguments, Direction::Subtract, false)
}

pub fn sub_function(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    change(
        context,
        args::object(arguments, 0),
        &arguments[1..],
        Direction::Subtract,
        true,
    )
}

fn change(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    direction: Direction,
    procedural: bool,
) -> NativeResult {
    let object = receiver(this)?;
    let mut date = Instant::read(object)?;
    let interval = Interval::read(args::object(arguments, 0).expect("interval argument"))?;
    if direction == Direction::Subtract && interval.special() {
        let reason = "Only non-special relative time specifications are supported for subtraction";
        if procedural {
            context.warning(format!("date_sub(): {reason}"));
            return Ok(Value::Object(object.clone()));
        }
        return Err(NativeError::throw(
            "DateInvalidOperationException",
            format!("{}::sub(): {reason}", Instant::base_class(object)),
        ));
    }
    apply(&mut date, interval, direction);
    date.changed(context, object)
}

fn apply(date: &mut Instant, interval: Interval, direction: Direction) {
    if interval.special() {
        apply_relative(date, interval.relative);
        return;
    }
    let positive = (interval.invert == 0) == (direction == Direction::Add);
    let sign: i64 = if positive { 1 } else { -1 };
    let [years, months, days, hours, minutes, seconds, micros] = interval.relative.delta;
    if interval.arithmetic == Arithmetic::Civil {
        let delta = interval
            .relative
            .delta
            .map(|field| field.wrapping_mul(sign));
        apply_relative(
            date,
            State {
                delta,
                relative: true,
                ..State::default()
            },
        );
        return;
    }
    if [years, months, days] != [0; 3] {
        let delta = [years, months, days, 0, 0, 0, 0].map(|field| field.wrapping_mul(sign));
        apply_relative(
            date,
            State {
                delta,
                relative: true,
                ..State::default()
            },
        );
    }
    let seconds = hours
        .wrapping_mul(3600)
        .wrapping_add(minutes.wrapping_mul(60))
        .wrapping_add(seconds)
        .wrapping_add(micros.div_euclid(1_000_000));
    date.timestamp = date.timestamp.wrapping_add(seconds.wrapping_mul(sign));
    date.civil_override = None;
    if micros != 0 {
        let base = date.format();
        let fraction = date.micros + micros.rem_euclid(1_000_000) * sign;
        date.wall(base.civil, fraction, Some(base.offset.daylight), false);
    }
}

pub fn apply_relative(date: &mut Instant, mut relative: State) {
    let base = date.format();
    relative.parsed.microsecond = Some(date.micros);
    date.apply(&relative, base.civil, Some(base.offset.daylight));
}

pub fn diff(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let date = Instant::read(receiver(this)?)?;
    let target = Instant::read(args::object(arguments, 0).expect("target date"))?;
    let absolute = args::bool(arguments, 1).unwrap_or(false);
    Interval::from_difference(interval_diff::difference(&date, &target, absolute)).make(context)
}
