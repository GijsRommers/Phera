//! Request-local zones: `date_default_timezone_set('Europe/Brussels')` never
//! changes another request, the host timezone, or environment variables.

use std::sync::Arc;

use phera_extension_api::{Context, NativeResult, Value, args};
use tz::{LocalTimeType, TimeZoneRef};

use crate::calendar::Civil;

#[derive(Clone, Debug)]
pub struct Zone {
    pub name: String,
    pub rules: TimeZoneRef<'static>,
    modern: Option<Arc<tz::TimeZone>>,
}

#[derive(Clone, Debug)]
pub struct Offset {
    pub seconds: i32,
    pub daylight: bool,
    pub abbreviation: String,
}

impl Default for Zone {
    fn default() -> Self {
        Self {
            name: "UTC".into(),
            rules: tzdb_data::time_zone::etc::UTC,
            modern: None,
        }
    }
}

impl Zone {
    pub fn named(name: &[u8]) -> Option<Self> {
        // PHP's timezone identifier lookup stops at NUL even though its strings do not.
        let name = name.split(|byte| *byte == 0).next()?;
        let rules = *tzdb_data::find_tz(name)?;
        Some(Self {
            name: String::from_utf8(name.to_vec()).ok()?,
            rules,
            modern: jiff_tzdb::get(std::str::from_utf8(name).ok()?)
                .and_then(|(_, bytes)| tz::TimeZone::from_tz_data(bytes).ok())
                .map(Arc::new),
        })
    }

    pub fn offset(&self, timestamp: i64) -> Offset {
        // IANA's main zones agree after 1970; backzone retains PHP's distinct
        // pre-1970 histories. Pin both bundles so host updates cannot change results.
        let rules = if (timestamp >= 0
            || matches!(
                self.name.to_ascii_lowercase().as_str(),
                "eire" | "europe/dublin"
            ))
            && !matches!(self.name.to_ascii_lowercase().as_str(), "africa/windhoek")
        {
            self.modern
                .as_ref()
                .map_or(self.rules, |zone| zone.as_ref().as_ref())
        } else {
            self.rules
        };
        if let Some(offset) = self.legacy_alias(timestamp) {
            return offset;
        }
        match rules.find_local_time_type(timestamp) {
            Ok(kind) => {
                let mut offset = Offset::from(kind);
                // Jiff distributes rearguard data (positive DST). PHP exposes
                // vanguard's negative DST flags for Morocco's Ramadan rules.
                if matches!(
                    self.name.to_ascii_lowercase().as_str(),
                    "africa/casablanca" | "africa/el_aaiun" | "eire" | "europe/dublin"
                ) && (matches!(
                    self.name.to_ascii_lowercase().as_str(),
                    "eire" | "europe/dublin"
                ) || timestamp < 1789866000)
                    && let Ok(historical) = self.rules.find_local_time_type(timestamp)
                    && historical.ut_offset() == offset.seconds
                {
                    offset.daylight = historical.is_dst();
                }
                offset
            }
            Err(_) => {
                if let Some(rule) = rules.extra_rule()
                    && let Some(kind) = crate::posix::offset(rule, timestamp)
                {
                    let mut offset = Offset::from(kind);
                    if matches!(
                        self.name.to_ascii_lowercase().as_str(),
                        "eire" | "europe/dublin"
                    ) {
                        offset.daylight = !offset.daylight;
                    }
                    return offset;
                }
                // timelib's next-year transitions overflow at PHP_INT_MAX.
                Offset {
                    seconds: 0,
                    daylight: false,
                    abbreviation: match self.name.to_ascii_uppercase().as_str() {
                        "EST5EDT" => "EDT",
                        "CST6CDT" => "CDT",
                        "MST7MDT" => "MDT",
                        "PST8PDT" => "PDT",
                        _ => self.rules.local_time_types()[0].time_zone_designation(),
                    }
                    .to_string(),
                }
            }
        }
    }

    fn legacy_alias(&self, timestamp: i64) -> Option<Offset> {
        let (seconds, abbreviation, summer) = match self.name.to_ascii_uppercase().as_str() {
            "EST5EDT" => (-18000, "EST", "EDT"),
            "CST6CDT" => (-21600, "CST", "CDT"),
            "MST7MDT" => (-25200, "MST", "MDT"),
            "PST8PDT" => (-28800, "PST", "PDT"),
            _ => return None,
        };
        if timestamp < 0 {
            let local = Civil::from_timestamp(timestamp.wrapping_add(i64::from(seconds)));
            let daylight = match local.year {
                1918 | 1919 | 1967..=1969 => {
                    let start_month = if local.year < 1920 { 3 } else { 4 };
                    let start = last_sunday(local.year, start_month);
                    let end = last_sunday(local.year, 10) - 3600;
                    (start..end).contains(&local.timestamp())
                }
                1942..=1945 => {
                    let start = Civil {
                        year: 1942,
                        month: 2,
                        day: 9,
                        hour: 2,
                        minute: 0,
                        second: 0,
                    }
                    .timestamp();
                    let end = Civil {
                        year: 1945,
                        month: 9,
                        day: 30,
                        hour: 1,
                        minute: 0,
                        second: 0,
                    }
                    .timestamp();
                    (start..end).contains(&local.timestamp())
                }
                _ => false,
            };
            Some(Offset {
                seconds: seconds + if daylight { 3600 } else { 0 },
                daylight,
                abbreviation: if daylight { summer } else { abbreviation }.into(),
            })
        } else {
            None
        }
    }

    pub fn local(&self, timestamp: i64) -> (Civil, Offset) {
        let offset = self.offset(timestamp);
        let civil = Civil::from_timestamp(timestamp.wrapping_add(i64::from(offset.seconds)));
        (civil, offset)
    }
}

impl From<&LocalTimeType> for Offset {
    fn from(kind: &LocalTimeType) -> Self {
        Self {
            seconds: kind.ut_offset(),
            daylight: kind.is_dst(),
            abbreviation: kind.time_zone_designation().to_owned(),
        }
    }
}

pub fn current(context: &mut dyn Context) -> Zone {
    context.state().get_or_default::<Zone>().clone()
}

pub fn get(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from(current(context).name))
}

pub fn set(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let bytes = args::bytes(arguments, 0);
    if let Some(zone) = Zone::named(bytes) {
        *context.state().get_or_default::<Zone>() = zone;
        Ok(Value::Bool(true))
    } else {
        let name =
            String::from_utf8_lossy(bytes.split(|byte| *byte == 0).next().unwrap_or_default());
        context.notice(format!(
            "date_default_timezone_set(): Timezone ID '{name}' is invalid"
        ));
        Ok(Value::Bool(false))
    }
}

pub fn version(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from("2026.5"))
}

/// The historical US rules used by PHP's four legacy POSIX identifiers.
fn last_sunday(year: i64, month: i64) -> i128 {
    let last = Civil {
        year,
        month,
        day: 31,
        hour: 2,
        minute: 0,
        second: 0,
    };
    let day = crate::calendar::month_days(year, month);
    let last = Civil { day, ..last };
    last.timestamp() - i128::from(last.weekday()) * 86400
}

impl Zone {
    pub(crate) fn alternate_rule(&self) -> Option<&tz::timezone::AlternateTime> {
        let rules = self.modern.as_ref().map_or(self.rules, |z| z.as_ref().as_ref());
        match rules.extra_rule()? {
            tz::timezone::TransitionRule::Alternate(rule) => Some(rule),
            _ => None,
        }
    }
    pub(crate) fn year_transitions(&self, year: i64) -> Vec<i64> {
        let Some(rule) = self.alternate_rule() else {return Vec::new()};
        let mut times = vec![
            crate::posix::transition(year,rule.dst_start(),rule.dst_start_time(),rule.std().ut_offset()),
            crate::posix::transition(year,rule.dst_end(),rule.dst_end_time(),rule.dst().ut_offset()),
        ];
        times.sort_unstable();
        times
    }
}
