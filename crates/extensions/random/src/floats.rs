//! `getFloat($min,$max)` uses PHP's gamma-section algorithm, not linear interpolation.
//! Ported from php-src/ext/random/gammasection.c (BSD-3-Clause, LICENSE-PHP).
//!
//! ```php
//! (new Random\Randomizer())->getFloat(0.0, 1.0);  // one of the doubles in [0, 1)
//! ```
//!
//! Every result is one end of the interval moved towards the other by a
//! whole number of steps of `gamma`, the gap between adjacent doubles at
//! the end farther from zero, so all of them are equally likely.

use phera_extension_api::{ArrayKey, ClassEntry, ClassKind, NativeError, Value};

use crate::engines::Engine;

pub(crate) const INTERVAL_BOUNDARY: ClassEntry = ClassEntry {
    kind: ClassKind::UnitEnum(&["ClosedOpen", "ClosedClosed", "OpenClosed", "OpenOpen"]),
    ..ClassEntry::class("Random\\IntervalBoundary")
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Boundary {
    ClosedOpen,
    ClosedClosed,
    OpenClosed,
    OpenOpen,
}

impl Boundary {
    pub(crate) fn argument(value: Option<&Value>) -> Result<Self, NativeError> {
        let Some(value) = value else {
            return Ok(Self::ClosedOpen);
        };
        if let Value::Object(object) = value
            && let Some(Value::String(case)) = object.get(&ArrayKey::str("name"))
        {
            let boundary = match case.as_bytes() {
                b"ClosedOpen" => Self::ClosedOpen,
                b"ClosedClosed" => Self::ClosedClosed,
                b"OpenClosed" => Self::OpenClosed,
                b"OpenOpen" => Self::OpenOpen,
                _ => return Err(NativeError::error("Invalid Random\\IntervalBoundary case")),
            };
            return Ok(boundary);
        }
        Err(NativeError::type_error(format!(
            "Random\\Randomizer::getFloat(): Argument #3 ($boundary) must be of type Random\\IntervalBoundary, {} given",
            crate::given(value)
        )))
    }
}

/// Draw a double by counting steps from the end of the interval farther from zero.
pub(crate) fn draw(
    engine: &mut Engine,
    min: f64,
    max: f64,
    boundary: Boundary,
) -> Result<f64, NativeError> {
    let gamma = if min.abs() > max.abs() {
        min.next_up() - min
    } else {
        max - max.next_down()
    };
    // How many steps fit, and the rounding error of that count.
    let distance = max / gamma - min / gamma;
    let error = if min.abs() <= max.abs() {
        -min / gamma - (distance - max / gamma)
    } else {
        max / gamma - (distance + min / gamma)
    };
    let ceil = distance.ceil();
    let hi = ceil as u64 + u64::from(distance == ceil && error > 0.0);
    if hi == 0 && matches!(boundary, Boundary::ClosedOpen | Boundary::OpenClosed) {
        return Ok(f64::NAN);
    }
    let (first, last, edge) = match (boundary, min.abs() <= max.abs()) {
        (Boundary::ClosedOpen, true) => (1, hi, Some(min)),
        (Boundary::ClosedOpen, false) => (0, hi - 1, None),
        (Boundary::ClosedClosed, true) => (0, hi, Some(min)),
        (Boundary::ClosedClosed, false) => (0, hi, Some(max)),
        (Boundary::OpenClosed, true) => (0, hi - 1, None),
        (Boundary::OpenClosed, false) => (1, hi, Some(max)),
        (Boundary::OpenOpen, _) if hi < 2 => {
            return Err(NativeError::value_error(
                "The given interval is empty, there are no floats between argument #1 ($min) and argument #2 ($max)",
            ));
        }
        (Boundary::OpenOpen, _) => (1, hi - 1, None),
    };
    let k = first + engine.range64(last - first)?;
    if k == last
        && let Some(edge) = edge
    {
        return Ok(edge);
    }
    // Steps are taken four at a time so that no product overflows.
    if min.abs() <= max.abs() {
        Ok(4.0 * (max * 0.25 - (k >> 2) as f64 * gamma) - (k & 3) as f64 * gamma)
    } else {
        Ok(4.0 * (min * 0.25 + (k >> 2) as f64 * gamma) + (k & 3) as f64 * gamma)
    }
}
