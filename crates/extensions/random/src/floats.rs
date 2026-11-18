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

use phera_extension_api::NativeError;

use crate::engines::Engine;

/// A double in `[min, max)`; both are finite and `min < max`.
pub fn closed_open(engine: &mut Engine, min: f64, max: f64) -> Result<f64, NativeError> {
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
    let k = 1 + engine.range64(hi - 1)?;
    // Steps are taken four at a time so that no product overflows.
    if min.abs() <= max.abs() {
        if k == hi {
            Ok(min)
        } else {
            Ok(4.0 * (max * 0.25 - (k >> 2) as f64 * gamma) - (k & 3) as f64 * gamma)
        }
    } else {
        let k = k - 1;
        Ok(4.0 * (min * 0.25 + (k >> 2) as f64 * gamma) + (k & 3) as f64 * gamma)
    }
}
