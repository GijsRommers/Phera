//! Parsed state retains missing fields and relative instructions until a base date is supplied.
use super::token::{DateToken, Special, TimeToken, WeekdayBehavior};
use crate::parse_format::result::{Parsed, ParsedZone};
use phera_extension_api::{ArrayKey, PhpArray, Value};

#[derive(Default, Debug)]
pub struct State {
    pub parsed: Parsed,
    pub have_date: bool,
    pub have_time: usize,
    pub have_zone: usize,
    pub relative: bool,
    pub delta: [i64; 7],
    pub weekday: Option<i64>,
    pub behavior: WeekdayBehavior,
    pub special: Special,
    pub month_edge: Option<bool>,
}
impl State {
    pub fn error(&mut self, position: usize, message: &str) {
        if self.parsed.errors.len() < 1000 {
            self.parsed.error(position, message);
        }
    }
    pub fn warning(&mut self, position: usize, message: &str) {
        if self.parsed.warnings.len() < 1000 {
            self.parsed.warnings.push((position, message.into()));
        }
    }
    pub fn midnight(&mut self) {
        self.have_time = 0;
        self.parsed.fields[3..].fill(Some(0));
        self.parsed.microsecond = Some(0);
    }
    pub fn date(&mut self, date: DateToken, position: usize) -> bool {
        if self.have_date {
            self.error(position, "Double date specification");
            return false;
        }
        self.have_date = true;
        if date.overflow {
            self.error(position, "Number out of range");
        }
        for (target, source) in self.parsed.fields[..3].iter_mut().zip(date.fields) {
            if source.is_some() {
                *target = source;
            }
        }
        if date.replace_year {
            self.parsed.fields[0] = date.fields[0];
        }
        true
    }
    pub fn time(&mut self, time: TimeToken, position: usize) -> bool {
        if self.have_time > 0 {
            self.error(position, "Double time specification");
            return false;
        }
        self.have_time = 1;
        for (target, source) in self.parsed.fields[3..].iter_mut().zip(time.fields) {
            *target = Some(source);
        }
        self.parsed.microsecond = Some(time.fields[3]);
        true
    }
    pub fn zone(&mut self, input: &[u8], position: usize, explicit: bool) {
        if explicit && !self.zone_available(position) {
            return;
        }
        let (zone, found) = crate::parse_format::zone::parse(input, &mut 0);
        if matches!(zone, ParsedZone::Invalid) {
            match &mut self.parsed.zone {
                Some(ParsedZone::Offset(seconds) | ParsedZone::Abbreviation { seconds, .. }) => {
                    *seconds = 0;
                }
                None => self.parsed.zone = Some(zone),
                _ => {}
            }
        } else {
            self.parsed.zone = Some(zone);
        }
        if !found {
            self.error(position, "The timezone could not be found in the database");
        }
    }
    pub fn zone_available(&mut self, position: usize) -> bool {
        self.have_zone += 1;
        match self.have_zone {
            1 => true,
            2 => {
                self.warning(position, "Double timezone specification");
                false
            }
            _ => {
                self.error(position, "Double timezone specification");
                false
            }
        }
    }
    pub fn finish(&mut self, position: usize) {
        if self.have_time > 0 {
            let [h, m, s] = self.parsed.fields[3..].try_into().unwrap();
            if h.is_some_and(|n| !(0..24).contains(&n))
                || m.is_some_and(|n| !(0..60).contains(&n))
                || s.is_some_and(|n| !(0..60).contains(&n))
            {
                self.warning(position, "The parsed time was invalid");
            }
        }
        if self.have_date {
            let [y, m, d] = self.parsed.fields[..3].try_into().unwrap();
            let (y, m, d) = (
                y.unwrap_or(-9999999),
                m.unwrap_or(-9999999),
                d.unwrap_or(-9999999),
            );
            if !(1..=12).contains(&m) || !(1..=crate::calendar::month_days(y, m)).contains(&d) {
                self.warning(position, "The parsed date was invalid");
            }
        }
    }
    pub fn array(&self) -> Value {
        let Value::Array(mut result) = self.parsed.array() else {
            unreachable!()
        };
        if self.relative {
            let mut relative = PhpArray::new();
            for (key, value) in ["year", "month", "day", "hour", "minute", "second"]
                .into_iter()
                .zip(self.delta)
            {
                relative.insert(ArrayKey::str(key), Value::Int(value));
            }
            if let Some(day) = self.weekday {
                relative.insert(ArrayKey::str("weekday"), Value::Int(day));
            }
            if let Special::Weekdays(count) = self.special {
                relative.insert(ArrayKey::str("weekdays"), Value::Int(count));
            }
            if let Some(first) = self.month_edge {
                relative.insert(
                    ArrayKey::str(if first {
                        "first_day_of_month"
                    } else {
                        "last_day_of_month"
                    }),
                    Value::Bool(true),
                );
            }
            std::rc::Rc::make_mut(&mut result)
                .insert(ArrayKey::str("relative"), Value::from(relative));
        }
        Value::Array(result)
    }
}
