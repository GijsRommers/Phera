//! Request-local zones: `date_default_timezone_set('Europe/Brussels')` never
//! changes another request, the host timezone, or environment variables.

use phera_extension_api::{Context, NativeResult, Value, args};
use tz::{LocalTimeType, TimeZoneRef};

use crate::calendar::Civil;

#[derive(Clone, Debug)]
pub struct Zone {
    pub name: String,
    pub rules: TimeZoneRef<'static>,
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
        })
    }

    pub fn offset(&self, timestamp: i64) -> Offset {
        match self.rules.find_local_time_type(timestamp) {
            Ok(kind) => Offset::from(kind),
            Err(_) => {
                // POSIX rules repeat on the Gregorian 400-year cycle. tz-rs's
                // evaluator uses i32 years; PHP's calendar does not.
                let civil = Civil::from_timestamp(timestamp);
                let mapped = Civil {
                    year: 2400 + civil.year.rem_euclid(400),
                    ..civil
                };
                let near_limit = civil.year == Civil::from_timestamp(i64::MAX).year;
                if !near_limit
                    && let Ok(kind) = self.rules.find_local_time_type(mapped.timestamp() as i64)
                {
                    return Offset::from(kind);
                }
                // timelib's next-year transitions overflow at PHP_INT_MAX.
                Offset {
                    seconds: 0,
                    daylight: false,
                    abbreviation: self.rules.local_time_types()[0]
                        .time_zone_designation()
                        .to_string(),
                }
            }
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
    Ok(Value::from(tzdb_data::VERSION))
}
