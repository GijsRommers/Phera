//! Parse results retain unset fields: `date_parse_from_format('Y', '2024')['month'] === false`.
use crate::calendar::month_days;
use phera_extension_api::{ArrayKey, PhpArray, Value};

#[derive(Clone, Debug)]
pub enum ParsedZone {
    Invalid,
    Offset(i64),
    Abbreviation {
        seconds: i64,
        daylight: bool,
        name: String,
    },
    Identifier {
        name: String,
        abbreviation: Option<String>,
    },
}

#[derive(Default, Clone, Debug)]
pub struct Parsed {
    pub fields: [Option<i64>; 6],
    pub microsecond: Option<i64>,
    pub zone: Option<ParsedZone>,
    pub weekday: Option<i64>,
    pub warnings: Vec<(usize, String)>,
    pub errors: Vec<(usize, String)>,
}

impl Parsed {
    pub fn error(&mut self, position: usize, message: &str) {
        self.errors.push((position, message.into()));
    }

    pub fn reset(&mut self, only_missing: bool) {
        for (field, default) in self.fields.iter_mut().zip([1970, 1, 1, 0, 0, 0]) {
            if !only_missing || field.is_none() {
                *field = Some(default);
            }
        }
        if !only_missing || self.microsecond.is_none() {
            self.microsecond = Some(0);
        }
        if !only_missing && let Some(ParsedZone::Identifier { name, .. }) = &mut self.zone {
            name.clear();
        }
    }

    pub fn finish(&mut self, position: usize) {
        if self.fields[3..].iter().any(Option::is_some) || self.microsecond.is_some() {
            for field in &mut self.fields[3..] {
                field.get_or_insert(0);
            }
            self.microsecond.get_or_insert(0);
        }
        if let [Some(hour), Some(minute), Some(second)] = &self.fields[3..]
            && (!(0..24).contains(hour) || !(0..60).contains(minute) || !(0..60).contains(second))
        {
            self.warnings
                .push((position, "The parsed time was invalid".into()));
        }
        if let [Some(year), Some(month), Some(day)] = &self.fields[..3]
            && (!(1..=12).contains(month) || !(1..=month_days(*year, *month)).contains(day))
        {
            self.warnings
                .push((position, "The parsed date was invalid".into()));
        }
    }

    pub fn array(&self) -> Value {
        let mut result = PhpArray::new();
        for (name, value) in ["year", "month", "day", "hour", "minute", "second"]
            .into_iter()
            .zip(self.fields)
        {
            result.insert(
                ArrayKey::str(name),
                value.map_or(Value::Bool(false), Value::Int),
            );
        }
        result.insert(
            ArrayKey::str("fraction"),
            self.microsecond.map_or(Value::Bool(false), |value| {
                Value::Float(value as f64 / 1_000_000.0)
            }),
        );
        for (key, value) in self.diagnostics().iter() {
            result.insert(key.clone(), value.clone());
        }
        result.insert(
            ArrayKey::str("is_localtime"),
            Value::Bool(self.zone.is_some()),
        );
        if let Some(zone) = &self.zone {
            zone.append(&mut result);
        }
        if let Some(weekday) = self.weekday {
            let mut relative = PhpArray::new();
            for name in ["year", "month", "day", "hour", "minute", "second"] {
                relative.insert(ArrayKey::str(name), Value::Int(0));
            }
            relative.insert(ArrayKey::str("weekday"), Value::Int(weekday));
            result.insert(ArrayKey::str("relative"), Value::from(relative));
        }
        Value::from(result)
    }

    pub fn diagnostics(&self) -> PhpArray {
        let mut result = PhpArray::new();
        for (name, messages) in [("warning", &self.warnings), ("error", &self.errors)] {
            result.insert(
                ArrayKey::str(format!("{name}_count")),
                Value::Int(messages.len() as i64),
            );
            let mut table = PhpArray::new();
            for (position, message) in messages {
                table.insert(
                    ArrayKey::Int(*position as i64),
                    Value::from(message.clone()),
                );
            }
            result.insert(ArrayKey::str(format!("{name}s")), Value::from(table));
        }
        result
    }
}

impl ParsedZone {
    fn append(&self, result: &mut PhpArray) {
        let kind = match self {
            Self::Invalid => 0,
            Self::Offset(_) => 1,
            Self::Abbreviation { .. } => 2,
            Self::Identifier { .. } => 3,
        };
        result.insert(ArrayKey::str("zone_type"), Value::Int(kind));
        match self {
            Self::Offset(seconds) => {
                result.insert(ArrayKey::str("zone"), Value::Int(*seconds));
                result.insert(ArrayKey::str("is_dst"), Value::Bool(false));
            }
            Self::Abbreviation {
                seconds,
                daylight,
                name,
            } => {
                result.insert(ArrayKey::str("zone"), Value::Int(*seconds));
                result.insert(ArrayKey::str("is_dst"), Value::Bool(*daylight));
                result.insert(ArrayKey::str("tz_abbr"), Value::from(name.clone()));
            }
            Self::Identifier { name, abbreviation } => {
                if let Some(abbreviation) = abbreviation {
                    result.insert(ArrayKey::str("tz_abbr"), Value::from(abbreviation.clone()));
                }
                if !name.is_empty() {
                    result.insert(ArrayKey::str("tz_id"), Value::from(name.clone()));
                }
            }
            Self::Invalid => {}
        }
    }
}
