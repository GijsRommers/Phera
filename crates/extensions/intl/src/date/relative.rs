//! Relative day styles use the formatter's local date and CLDR day names.
//!
//! ```php
//! $f = new IntlDateFormatter('fr_FR', IntlDateFormatter::RELATIVE_MEDIUM, -1, 'UTC');
//! $f->format(time()); // "aujourd’hui"
//! ```

use std::rc::Rc;

use icu_locale::Locale;
use phera_ext_date::ZoneKind;

use super::{Style, pattern, render::Renderer};
use crate::timezone::clock_millis;

#[derive(Clone)]
pub(super) struct Relative {
    join: &'static str,
    terms: Rc<[&'static str]>,
    clock: Option<Renderer>,
}

impl Relative {
    pub fn new(locale: &Locale, date_type: i64, time: Style) -> Option<Self> {
        if !matches!(date_type, 128..=131) {
            return None;
        }
        let id = locale.id.to_string();
        let mut candidate = id.as_str();
        let row = loop {
            if let Some(row) = lookup(candidate) {
                break row;
            }
            let Some((parent, _)) = candidate.rsplit_once('-') else {
                break lookup("en")?;
            };
            candidate = parent;
        };
        let index = Style::parse(date_type)?.index()?;
        let mut fields = row.split('\t');
        let join = fields.nth(index)?;
        let terms = fields.skip(3 - index).collect::<Vec<_>>();
        let clock = if time.index().is_some() {
            Renderer::new(locale, &pattern::select(locale, Style::None, time), 1)
        } else {
            None
        };
        Some(Self {
            join,
            terms: Rc::from(terms),
            clock,
        })
    }

    pub fn pattern(&self, locale: &Locale, date: Style, time: Style) -> String {
        pattern::select_with_join(locale, date, time, Some(self.join))
    }

    pub fn format(&self, timestamp: i64, micros: i64, zone: &ZoneKind) -> Option<String> {
        let day = |seconds| {
            let offset = i64::from(zone.offset(seconds).seconds);
            seconds.saturating_add(offset).div_euclid(86_400)
        };
        let offset = day(timestamp) - day(clock_millis().div_euclid(1000));
        let term = self.terms.iter().find_map(|field| {
            let (key, value) = field.split_once(':')?;
            (key.parse::<i64>().ok()? == offset).then_some(value)
        })?;
        if let Some(clock) = &self.clock {
            let clock = clock.format(timestamp, micros, zone)?;
            Some(self.join.replace("{1}", term).replace("{0}", &clock))
        } else {
            Some(term.to_owned())
        }
    }
}

fn lookup(locale: &str) -> Option<&'static str> {
    include_str!("../../data/cldr-relative-days.tsv")
        .lines()
        .find_map(|line| {
            let (key, fields) = line.split_once('\t')?;
            (key == locale).then_some(fields)
        })
}
