//! Numeric generation for php-src ext/standard/array.c: PHP_FUNCTION(range).

use phera_extension_api::{NativeError, NativeResult, PhpArray, Value};

use super::range::Step;
use super::range_endpoint::{Endpoint, error};

pub(super) enum RangeKind {
    Integer,
    Floating,
    Character,
}

enum Direction {
    Increasing,
    Decreasing,
}

pub(super) struct RangeValues {
    start: Endpoint,
    end: Endpoint,
    step: Step,
    kind: RangeKind,
    direction: Direction,
}

impl RangeValues {
    /// Integer endpoints must be compared before conversion to binary64.
    pub(super) fn new(start: Endpoint, end: Endpoint, step: Step, kind: RangeKind) -> Self {
        let increasing = match kind {
            RangeKind::Floating => start.number < end.number,
            _ => start.integer < end.integer,
        };
        Self {
            start,
            end,
            step,
            kind,
            direction: if increasing {
                Direction::Increasing
            } else {
                Direction::Decreasing
            },
        }
    }

    fn integer_distance(&self) -> u128 {
        (i128::from(self.end.integer) - i128::from(self.start.integer)).unsigned_abs()
    }

    fn distance(&self) -> f64 {
        match self.kind {
            RangeKind::Floating => (self.end.number - self.start.number).abs(),
            _ => self.integer_distance() as f64,
        }
    }

    /// Constant ranges accept negative steps; increasing ranges reject them first.
    fn count(&self) -> Result<u64, NativeError> {
        if matches!(self.direction, Direction::Increasing) && self.step.signed_step < 0.0 {
            return Err(error(
                3,
                "step",
                "must be greater than 0 for increasing ranges",
            ));
        }
        let equal = match self.kind {
            RangeKind::Floating => self.start.number == self.end.number,
            _ => self.start.integer == self.end.integer,
        };
        let distance = self.distance();
        if !equal && distance < self.step.step {
            return Err(error(
                3,
                "step",
                "must be less than the range spanned by argument #1 ($start) and argument #2 ($end)",
            ));
        }
        let count = if equal {
            1.0
        } else {
            match self.kind {
                RangeKind::Floating => (distance / self.step.step + 1.0).round(),
                _ => (self.integer_distance() / u128::from(self.step.integer_step) + 1) as f64,
            }
        };
        if count >= 1_073_741_824.0 {
            return Err(self.size_error());
        }
        Ok(count as u64)
    }

    /// PHP reports different size calculations for floating and integer ranges.
    fn size_error(&self) -> NativeError {
        let start = &self.start;
        let end = &self.end;
        let step = self.step.step;
        let low = start.number.min(end.number);
        let high = start.number.max(end.number);
        let message = match self.kind {
            RangeKind::Floating => format!(
                "The supplied range exceeds the maximum array size by {:.1} elements: start={low:.1}, end={high:.1}, step={step:.1}. Max size: 1073741824",
                self.distance() / step + 1.0 - 1_073_741_824.0
            ),
            _ => {
                let calculated = self.integer_distance() / u128::from(self.step.integer_step);
                format!(
                    "The supplied range exceeds the maximum array size by {} elements: start={}, end={}, step={}. Calculated size: {calculated}. Maximum size: 1073741824.",
                    calculated.saturating_sub(1_073_741_823),
                    start.integer.min(end.integer),
                    start.integer.max(end.integer),
                    self.step.integer_step
                )
            }
        };
        NativeError::value_error(message)
    }

    /// Compute every float from the original start to avoid cumulative rounding drift.
    fn value_at(&self, index: u64) -> Value {
        match self.kind {
            RangeKind::Floating => Value::Float(if index == 0 {
                self.start.number
            } else {
                match self.direction {
                    Direction::Increasing => self.start.number + index as f64 * self.step.step,
                    Direction::Decreasing => self.start.number - index as f64 * self.step.step,
                }
            }),
            _ => {
                let delta = i128::from(index) * i128::from(self.step.integer_step);
                let integer = match self.direction {
                    Direction::Increasing => i128::from(self.start.integer) + delta,
                    Direction::Decreasing => i128::from(self.start.integer) - delta,
                };
                match self.kind {
                    RangeKind::Character => Value::from([integer as u8].as_slice()),
                    _ => Value::Int(integer as i64),
                }
            }
        }
    }

    /// Rounded float counts can overrun the endpoint; PHP stops before that value.
    pub(super) fn collect(self) -> NativeResult {
        let count = self.count()?;
        let values = (0..count)
            .map(|index| self.value_at(index))
            .take_while(|value| match value {
                Value::Float(number) => !match self.direction {
                    Direction::Increasing => *number > self.end.number,
                    Direction::Decreasing => *number < self.end.number,
                },
                _ => true,
            });
        Ok(Value::from(PhpArray::from_values(values, count as usize)))
    }
}
