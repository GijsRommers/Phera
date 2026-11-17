//! `EDT` stays four hours west of UTC in winter, while an identifier follows its rules.
use crate::{
    calendar::Civil,
    format::DateFormat,
    parse_format::result::ParsedZone,
    zone::{Offset, Zone},
};

#[derive(Clone, Debug)]
pub enum ZoneKind {
    Identifier(Zone),
    Abbreviation(Offset),
    Fixed(i32),
}

impl ZoneKind {
    pub fn parsed(zone: &ParsedZone) -> Option<Self> {
        Some(match zone {
            ParsedZone::Invalid => return None,
            ParsedZone::Identifier { name, .. } => Self::Identifier(Zone::named(name.as_bytes())?),
            ParsedZone::Offset(seconds) => Self::Fixed(*seconds as i32),
            ParsedZone::Abbreviation {
                seconds,
                daylight,
                name,
            } => Self::Abbreviation(Offset {
                seconds: *seconds as i32 + i32::from(*daylight) * 3600,
                daylight: *daylight,
                abbreviation: name.clone(),
            }),
        })
    }

    pub fn kind(&self) -> i64 {
        match self {
            Self::Fixed(_) => 1,
            Self::Abbreviation(_) => 2,
            Self::Identifier(_) => 3,
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Identifier(z) => z.name.clone(),
            Self::Abbreviation(offset) => offset.abbreviation.clone(),
            Self::Fixed(s) => {
                let sign = if *s < 0 { '-' } else { '+' };
                let n = s.unsigned_abs();
                let mut name = format!("{sign}{:02}:{:02}", n / 3600, n / 60 % 60);
                if n % 60 != 0 {
                    name.push_str(&format!(":{:02}", n % 60));
                }
                name
            }
        }
    }

    pub fn offset(&self, timestamp: i64) -> Offset {
        match self {
            Self::Identifier(z) => z.offset(timestamp),
            Self::Abbreviation(o) => o.clone(),
            Self::Fixed(s) => Offset {
                seconds: *s,
                daylight: false,
                abbreviation: format!(
                    "GMT{}{:02}{:02}",
                    if *s < 0 { '-' } else { '+' },
                    s.unsigned_abs() / 3600,
                    s.unsigned_abs() / 60 % 60
                ),
            },
        }
    }

    pub fn format(&self, timestamp: i64, microsecond: i64) -> DateFormat {
        let offset = self.offset(timestamp);
        DateFormat {
            civil: Civil::from_timestamp(timestamp.wrapping_add(i64::from(offset.seconds))),
            offset,
            timestamp,
            microsecond: microsecond as u32,
            zone: self.name(),
            gmt: false,
        }
    }

    pub fn resolve(&self, civil: Civil, daylight: Option<bool>) -> i64 {
        match self {
            Self::Identifier(z) => super::wall_time::resolve(z, civil, daylight),
            _ => crate::normalize::timestamp(civil).wrapping_sub(i64::from(self.offset(0).seconds)),
        }
    }
}
