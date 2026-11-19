//! Formatting keeps the Unix instant and resolves the requested timezone first.
//!
//! ```php
//! $f = new IntlDateFormatter("en_US", -1, -1, "Europe/Brussels", 1, "HH:mm");
//! $f->format(0); // "01:00"
//! ```

use std::{fmt, rc::Rc};

use icu_calendar::{Date, Gregorian};
use icu_datetime::pattern::{DateTimePattern, FixedCalendarDateTimeNames};
use icu_datetime::{
    DateTimeFormatter,
    fieldsets::{YMDT, enums::CompositeFieldSet},
};
use icu_locale::Locale;
use icu_time::zone::{TimeZone, UtcOffset};
use icu_time::{DateTime, Time, ZonedDateTime};
use phera_ext_date::{ZoneKind, calendar::Civil};
use writeable::{Part, PartsWrite, TryWriteable, Writeable};

#[derive(Clone)]
pub enum Renderer {
    Pattern {
        pattern: Rc<DateTimePattern>,
        arabic: bool,
        literal_fraction: bool,
        names: Rc<FixedCalendarDateTimeNames<Gregorian, CompositeFieldSet>>,
    },
    Traditional(Rc<DateTimeFormatter<YMDT>>),
}

impl Renderer {
    pub fn new(locale: &Locale, pattern: &str, calendar: i64) -> Option<Self> {
        let mut locale = locale.clone();
        if calendar == 0 && is_non_gregorian(&locale) {
            let formatter = DateTimeFormatter::try_new(locale.into(), YMDT::medium()).ok()?;
            return Some(Self::Traditional(Rc::new(formatter)));
        }
        locale
            .extensions
            .unicode
            .keywords
            .remove(icu_locale::extensions::unicode::key!("ca"));
        let arabic = locale.id.language.as_str() == "ar";
        let literal_fraction = pattern.contains(".S");
        let pattern = pattern.parse::<DateTimePattern>().ok()?;
        let mut names =
            FixedCalendarDateTimeNames::<Gregorian, CompositeFieldSet>::try_new(locale.into())
                .ok()?;
        names.include_for_pattern(&pattern).ok()?;
        Some(Self::Pattern {
            pattern: Rc::new(pattern),
            arabic,
            literal_fraction,
            names: Rc::new(names),
        })
    }

    pub fn format(&self, seconds: i64, micros: i64, zone: &ZoneKind) -> Option<String> {
        let offset = zone.offset(seconds);
        let civil = Civil::from_timestamp(seconds.checked_add(i64::from(offset.seconds))?);
        let date = Date::try_new_gregorian(
            i32::try_from(civil.year).ok()?,
            civil.month as u8,
            civil.day as u8,
        )
        .ok()?;
        let time = Time::try_new(
            civil.hour as u8,
            civil.minute as u8,
            civil.second as u8,
            u32::try_from(micros * 1000).ok()?,
        )
        .ok()?;
        match self {
            Self::Pattern {
                pattern,
                names,
                arabic,
                literal_fraction,
            } => {
                let name = zone.name();
                let timezone = TimeZone::from_iana_id(&name)
                    .with_offset(UtcOffset::try_from_seconds(offset.seconds).ok())
                    .at_date_time(DateTime { date, time });
                let value = ZonedDateTime {
                    date,
                    time,
                    zone: timezone,
                };
                let formatter = names.with_pattern_unchecked(pattern);
                let formatted = formatter.format(&value);
                if *arabic || *literal_fraction {
                    let mut sink = PhpDateParts {
                        strip_arabic_mark: *arabic,
                        literal_fraction: *literal_fraction,
                        ..Default::default()
                    };
                    formatted.try_write_to_parts(&mut sink).ok()?.ok()?;
                    Some(sink.output)
                } else {
                    formatted
                        .try_write_to_string()
                        .ok()
                        .map(|text| text.into_owned())
                }
            }
            Self::Traditional(formatter) => Some(
                formatter
                    .format(&DateTime { date, time })
                    .write_to_string()
                    .into_owned(),
            ),
        }
    }
}

fn is_non_gregorian(locale: &Locale) -> bool {
    if let Some(calendar) = locale
        .extensions
        .unicode
        .keywords
        .get(&icu_locale::extensions::unicode::key!("ca"))
    {
        return calendar.to_string() != "gregory";
    }
    matches!(locale.id.language.as_str(), "th" | "fa")
        || locale
            .id
            .region
            .is_some_and(|region| region.as_str() == "SA")
}

/// Preserve punctuation PHP renders literally within its date format fields.
#[derive(Default)]
struct PhpDateParts {
    output: String,
    strip_arabic_mark: bool,
    literal_fraction: bool,
    in_zone_name: bool,
    in_seconds: bool,
}

impl fmt::Write for PhpDateParts {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.strip_arabic_mark && self.in_zone_name {
            self.output
                .extend(text.chars().filter(|&scalar| scalar != '\u{061c}'));
        } else {
            self.output.push_str(text);
        }
        Ok(())
    }
}

impl PartsWrite for PhpDateParts {
    type SubPartsWrite = Self;

    fn with_part(
        &mut self,
        part: Part,
        mut write: impl FnMut(&mut Self) -> fmt::Result,
    ) -> fmt::Result {
        if self.literal_fraction && self.in_seconds && part == icu_decimal::parts::DECIMAL {
            self.output.push('.');
            return Ok(());
        }
        let previous = (self.in_zone_name, self.in_seconds);
        self.in_zone_name |= part == icu_datetime::parts::TIME_ZONE_NAME;
        self.in_seconds |= part == icu_datetime::parts::SECOND;
        let result = write(self);
        (self.in_zone_name, self.in_seconds) = previous;
        result
    }
}
