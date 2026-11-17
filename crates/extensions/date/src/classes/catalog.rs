//! `DateTimeZone::listIdentifiers(DateTimeZone::PER_COUNTRY, 'BE')` lists Brussels.

use std::sync::LazyLock;

use phera_extension_api::{NativeError, NativeResult, PhpArray, Value, args};

use super::assoc;
use crate::zone::Offset;

pub const GROUPS: [&str; 11] = [
    "Africa",
    "America",
    "Antarctica",
    "Arctic",
    "Asia",
    "Atlantic",
    "Australia",
    "Europe",
    "Indian",
    "Pacific",
    "UTC",
];
pub const ALL: i64 = 2047;
pub const ALL_WITH_BC: i64 = 4095;
pub const PER_COUNTRY: i64 = 4096;

pub struct Location {
    name: &'static str,
    primary: bool,
    country: &'static str,
    latitude: f64,
    longitude: f64,
    times: &'static [u8],
    indices: &'static [u8],
    types: &'static [u8],
    abbreviations: &'static [u8],
    pub posix: &'static str,
    comments: &'static str,
}

static CATALOG: LazyLock<Vec<Location>> = LazyLock::new(|| {
    include_str!("../../data/locations.tsv")
        .lines()
        .map(location_record)
        .collect()
});

fn location_record(line: &'static str) -> Location {
    let fields: Vec<_> = line.split('\t').collect();
    let first: usize = fields[5].parse().expect("timezone data position");
    let count: usize = fields[6].parse().expect("timezone transition count");
    let types: usize = fields[7].parse().expect("timezone type count");
    let characters: usize = fields[8].parse().expect("timezone abbreviation length");
    let data = include_bytes!("../../data/transitions.bin");
    let indices = first + 8 * count;
    let offsets = indices + count;
    let abbreviations = offsets + 6 * types;
    Location {
        name: fields[0],
        primary: fields[1] == "1",
        country: fields[2],
        latitude: fields[3].parse::<f64>().expect("latitude") / 100_000.0 - 90.0,
        longitude: fields[4].parse::<f64>().expect("longitude") / 100_000.0 - 180.0,
        times: &data[first..indices],
        indices: &data[indices..offsets],
        types: &data[offsets..abbreviations],
        abbreviations: &data[abbreviations..abbreviations + characters],
        posix: fields.get(9).copied().unwrap_or_default(),
        comments: fields.get(10).copied().unwrap_or_default(),
    }
}

impl Location {
    pub fn find(name: &str) -> Option<&'static Self> {
        CATALOG
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    pub fn transitions(&self) -> impl DoubleEndedIterator<Item = (i64, usize)> + '_ {
        self.times
            .chunks_exact(8)
            .zip(self.indices)
            .map(|(bytes, index)| {
                (
                    i64::from_be_bytes(bytes.try_into().expect("transition")),
                    usize::from(*index),
                )
            })
    }

    pub fn offset(&self, index: usize) -> Offset {
        let kind = &self.types[index * 6..index * 6 + 6];
        let abbreviation = &self.abbreviations[usize::from(kind[5])..];
        let abbreviation = abbreviation
            .split(|byte| *byte == 0)
            .next()
            .unwrap_or_default();
        Offset {
            seconds: i32::from_be_bytes(kind[..4].try_into().expect("offset seconds")),
            daylight: kind[4] != 0,
            abbreviation: std::str::from_utf8(abbreviation)
                .expect("abbreviation")
                .into(),
        }
    }

    pub fn value(&self) -> Value {
        assoc([
            ("country_code", self.country.into()),
            ("latitude", Value::Float(self.latitude)),
            ("longitude", Value::Float(self.longitude)),
            ("comments", self.comments.into()),
        ])
        .into()
    }
}

pub fn identifiers(arguments: &[Value], site: &str) -> NativeResult {
    let group = args::int(arguments, 0).unwrap_or(ALL);
    let country = args::bytes(arguments, 1);
    if group == PER_COUNTRY && country.len() != 2 {
        let requirement = "must be a two-letter ISO 3166-1 compatible country code";
        let condition = "when argument #1 ($timezoneGroup) is DateTimeZone::PER_COUNTRY";
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #2 ($countryCode) {requirement} {condition}"
        )));
    }
    let mut result = PhpArray::new();
    for location in CATALOG.iter() {
        let group_name = location.name.split('/').next().unwrap_or_default();
        let include = match group {
            PER_COUNTRY => location.country.as_bytes() == country,
            ALL_WITH_BC => true,
            _ => {
                location.primary
                    && GROUPS
                        .iter()
                        .enumerate()
                        .any(|(index, name)| group & (1 << index) != 0 && *name == group_name)
            }
        };
        if include {
            result.push(location.name.into())?;
        }
    }
    Ok(result.into())
}
