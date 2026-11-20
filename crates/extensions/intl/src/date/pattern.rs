//! PHP date styles resolve to a locale's CLDR date, clock and join patterns.
//!
//! ```php
//! (new IntlDateFormatter('en_US', IntlDateFormatter::SHORT,
//!     IntlDateFormatter::NONE))->getPattern(); // "M/d/yy"
//! ```

use icu_locale::Locale;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Full,
    Long,
    Medium,
    Short,
    None,
    Pattern,
}

impl Style {
    pub fn parse(value: i64) -> Option<Self> {
        match value {
            0 | 128 => Some(Self::Full),
            1 | 129 => Some(Self::Long),
            2 | 130 => Some(Self::Medium),
            3 | 131 => Some(Self::Short),
            -1 => Some(Self::None),
            -2 => Some(Self::Pattern),
            _ => None,
        }
    }

    pub fn index(self) -> Option<usize> {
        match self {
            Self::Full => Some(0),
            Self::Long => Some(1),
            Self::Medium => Some(2),
            Self::Short => Some(3),
            Self::None | Self::Pattern => None,
        }
    }
}

pub fn select(locale: &Locale, date: Style, time: Style) -> String {
    select_with_join(locale, date, time, None)
}

pub fn select_with_join(locale: &Locale, date: Style, time: Style, join: Option<&str>) -> String {
    let Some(fields) = fields(locale) else {
        return "yMMdd hh:mm a".to_owned();
    };
    let fields: Vec<_> = fields.split('\t').collect();
    let date_text = date.index().and_then(|index| fields.get(index)).copied();
    let time_text = time
        .index()
        .and_then(|index| fields.get(4 + index))
        .copied();
    match (date_text, time_text) {
        (Some(date_text), Some(time_text)) => {
            let join = join.unwrap_or_else(|| join_field(&fields, date));
            join.replace("{1}", date_text).replace("{0}", time_text)
        }
        (Some(date), None) => date.to_owned(),
        (None, Some(time)) => time.to_owned(),
        (None, None) => "yMMdd hh:mm a".to_owned(),
    }
}

pub fn join(locale: &Locale, date: Style) -> &'static str {
    let fields: Vec<_> = fields(locale).unwrap_or_default().split('\t').collect();
    join_field(&fields, date)
}

fn join_field<'a>(fields: &[&'a str], date: Style) -> &'a str {
    date.index()
        .and_then(|index| fields.get(8 + index))
        .copied()
        .unwrap_or("{1}, {0}")
}

fn fields(locale: &Locale) -> Option<&'static str> {
    let identifier = locale.id.to_string();
    let windows = |tag: &str| {
        include_str!("../../data/cldr-gregorian-styles.tsv")
            .lines()
            .find_map(|row| {
                let (locale, fields) = row.split_once('\t')?;
                (tag == locale).then_some(fields)
            })
    };
    windows(&identifier)
        .or_else(|| windows(locale.id.language.as_str()))
        .or_else(|| windows("en"))
}

pub fn actual_locale(locale: &Locale) -> String {
    let id = locale.id.to_string();
    let Some((language, _)) = id.split_once('-') else {
        return id;
    };
    let entries = include_str!("../../data/cldr-gregorian-styles.tsv");
    let mut language_patterns = None;
    let mut region_patterns = None;
    for line in entries.lines() {
        if let Some((key, patterns)) = line.split_once('\t') {
            if key == language {
                language_patterns = Some(patterns);
            }
            if key == id {
                region_patterns = Some(patterns);
            }
        }
    }
    if region_patterns.is_some() && region_patterns != language_patterns {
        id.replace('-', "_")
    } else {
        language.to_owned()
    }
}
