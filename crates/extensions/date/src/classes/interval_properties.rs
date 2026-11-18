//! `$i->d = '3.5'` stores an integer. `from_string` belongs to the debug representation.

use phera_extension_api::value::{to_float, to_int};
use phera_extension_api::{
    Context, NativeError, NativePropertyAccess, NativePropertyMutation, NativePropertyRead, Object,
    Value,
};

use super::interval::{FIELDS, Interval, fraction_micros, optional_integer};

pub(super) const ACCESS: NativePropertyAccess = NativePropertyAccess { read, mutate };

enum Field {
    Unit(usize),
    Fraction,
    Invert,
    Days,
}

impl Field {
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "f" => Self::Fraction,
            "invert" => Self::Invert,
            "days" => Self::Days,
            _ => Self::Unit(FIELDS.iter().position(|field| *field == name)?),
        })
    }

    fn read(self, interval: &Interval) -> Value {
        match self {
            Self::Unit(index) => optional_integer(interval.relative.delta[index]),
            Self::Fraction if interval.relative.delta[6] == -1_000_000 => Value::Int(-1),
            Self::Fraction => Value::Float(interval.relative.delta[6] as f64 / 1_000_000.0),
            Self::Invert => optional_integer(interval.invert),
            Self::Days => interval.days.map_or(Value::Bool(false), optional_integer),
        }
    }
}

fn read(
    _: &mut dyn Context,
    object: &Object,
    name: &str,
    mode: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    let Some(field) = Field::named(name) else {
        return Ok(None);
    };
    let state = object.native.borrow();
    let Some(interval) = state
        .as_ref()
        .and_then(|state| state.downcast_ref::<Interval>())
    else {
        return Ok(None);
    };
    if matches!(mode, NativePropertyRead::Exists) {
        return Ok(Some(Value::Null));
    }
    Ok(Some(field.read(interval)))
}

fn mutate(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    change: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    let Some(field) = Field::named(name) else {
        return Ok(false);
    };
    let mut state = object.native.borrow_mut();
    let Some(interval) = state
        .as_mut()
        .and_then(|state| state.downcast_mut::<Interval>())
    else {
        return Ok(false);
    };
    let value = match change {
        NativePropertyMutation::Assign(value) => value,
        NativePropertyMutation::Reference => return Ok(true),
        NativePropertyMutation::Bind => {
            return Err(NativeError::error(
                "Cannot assign by reference to overloaded object",
            ));
        }
        NativePropertyMutation::Indirect if name != "days" => {
            return Err(NativeError::error("Cannot use a scalar value as an array"));
        }
        _ => return Ok(false),
    };
    match field {
        Field::Unit(index) => {
            interval.relative.delta[index] = to_int(value, context.diagnostics())?
        }
        Field::Fraction => {
            let value = to_float(value, context.diagnostics())?;
            interval.relative.delta[6] = fraction_micros(value);
        }
        Field::Invert => interval.invert = i64::from(to_int(value, context.diagnostics())? as i32),
        Field::Days => return Ok(false), // PHP stores an unrelated dynamic property on write.
    }
    Ok(true)
}
