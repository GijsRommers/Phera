//! Every read of `$period->start` constructs a fresh snapshot of the native date.

use phera_extension_api::{
    Context, NativeError, NativePropertyAccess, NativePropertyMutation, NativePropertyRead,
    NativeResult, Object, Value,
};

use super::period::{CLASS, Period};

pub(super) const ACCESS: NativePropertyAccess = NativePropertyAccess { read, mutate };

#[derive(Clone, Copy)]
pub(super) enum Field {
    Start,
    Current,
    End,
    Interval,
    Recurrences,
    IncludeStart,
    IncludeEnd,
}

impl Field {
    pub const ALL: [Self; 7] = [
        Self::Start,
        Self::Current,
        Self::End,
        Self::Interval,
        Self::Recurrences,
        Self::IncludeStart,
        Self::IncludeEnd,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Current => "current",
            Self::End => "end",
            Self::Interval => "interval",
            Self::Recurrences => "recurrences",
            Self::IncludeStart => "include_start_date",
            Self::IncludeEnd => "include_end_date",
        }
    }

    pub fn value(self, context: &mut dyn Context, period: &Period) -> NativeResult {
        match self {
            Self::Start => period.make_date(context, period.start.clone()),
            Self::Current => period.make_date(context, period.current.clone()),
            Self::End => period.make_date(context, period.end.clone()),
            Self::Interval => period.make_interval(context),
            Self::Recurrences => Ok(Value::Int(period.recurrences)),
            Self::IncludeStart => Ok(Value::Bool(period.include_start)),
            Self::IncludeEnd => Ok(Value::Bool(period.include_end)),
        }
    }
}

fn read(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    mode: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    let Some(field) = Field::ALL.into_iter().find(|field| field.name() == name) else {
        return Ok(None);
    };
    if matches!(mode, NativePropertyRead::Exists) {
        return Ok(Some(Value::Null));
    }
    let period = Period::read(object).unwrap_or_default();
    if matches!(mode, NativePropertyRead::Isset) && !period.initialized {
        return Ok(Some(Value::Null));
    }
    field.value(context, &period).map(Some)
}

fn mutate(
    _: &mut dyn Context,
    object: &Object,
    name: &str,
    change: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    if !Field::ALL.into_iter().any(|field| field.name() == name) {
        return Ok(false);
    }
    let message = match change {
        NativePropertyMutation::Unset => format!("Cannot unset {}::${name}", object.class.name()),
        _ => format!("Cannot modify readonly property {CLASS}::${name}"),
    };
    Err(NativeError::error(message))
}
