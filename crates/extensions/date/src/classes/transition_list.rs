//! `getTransitions($from, $to)` starts with a snapshot at the requested beginning.

use phera_extension_api::{NativeError, NativeResult, PhpArray, Value};
use tz::timezone::{AlternateTime, TransitionRule};

use super::{assoc, catalog::Location};
use crate::{
    calendar::Civil,
    format::DateFormat,
    posix,
    zone::{Offset, Zone},
};

pub fn list(zone: &Zone, begin: i64, end: i64) -> NativeResult {
    let Some(location) = Location::find(&zone.name) else {
        return Ok(Value::Bool(false));
    };
    let last = location.transitions().next_back().map(|entry| entry.0);
    let rule = posix_rule(location);
    let snapshot = match (last, rule.as_ref()) {
        (Some(last), Some(TransitionRule::Alternate(_))) if begin >= last => {
            posix::offset(rule.as_ref().expect("alternate rule"), begin)
                .map_or_else(|| zone.offset(begin), Offset::from)
        }
        _ => location.offset(
            location
                .transitions()
                .rev()
                .find(|(ts, _)| *ts <= begin)
                .map_or(0, |(_, kind)| kind),
        ),
    };
    let mut result = PhpArray::new();
    let utc = Zone::default();
    append(&mut result, &utc, begin, snapshot)?;
    for (timestamp, kind) in location.transitions().filter(|(ts, _)| *ts > begin) {
        if timestamp >= end {
            return Ok(result.into());
        }
        append(&mut result, &utc, timestamp, location.offset(kind))?;
    }
    if let (Some(last), Some(TransitionRule::Alternate(rule))) = (last, rule) {
        for year in Civil::from_timestamp(begin.max(last)).year..=Civil::from_timestamp(end).year {
            for (timestamp, offset) in year_transitions(&rule, year) {
                if timestamp > last && timestamp >= begin && timestamp <= end {
                    append(&mut result, &utc, timestamp, offset)?;
                }
            }
        }
    }
    Ok(result.into())
}

fn append(
    result: &mut PhpArray,
    utc: &Zone,
    timestamp: i64,
    offset: Offset,
) -> Result<(), NativeError> {
    let time = DateFormat::new(timestamp, utc, false).render(b"x-m-d\\TH:i:sP");
    result.push(
        assoc([
            ("ts", Value::Int(timestamp)),
            ("time", time.into()),
            ("offset", Value::Int(i64::from(offset.seconds))),
            ("isdst", Value::Bool(offset.daylight)),
            ("abbr", offset.abbreviation.into()),
        ])
        .into(),
    )?;
    Ok(())
}

fn posix_rule(location: &Location) -> Option<TransitionRule> {
    if location.posix.is_empty() {
        return None;
    }
    // A transition-free TZif prefix lets tz-rs parse the embedded extended POSIX footer.
    let mut section = [0; 54];
    section[..5].copy_from_slice(b"TZif3");
    section[39] = 1; // One time type at offset zero.
    section[43] = 4; // Four bytes in the abbreviation table.
    section[50..].copy_from_slice(b"UTC\0");
    let mut tzif = section.repeat(2);
    tzif.push(b'\n');
    tzif.extend_from_slice(location.posix.as_bytes());
    tzif.push(b'\n');
    *tz::TimeZone::from_tz_data(&tzif)
        .ok()?
        .as_ref()
        .extra_rule()
}

fn year_transitions(rule: &AlternateTime, year: i64) -> [(i64, Offset); 2] {
    let start = posix::transition(
        year,
        rule.dst_start(),
        rule.dst_start_time(),
        rule.std().ut_offset(),
    );
    let end = posix::transition(
        year,
        rule.dst_end(),
        rule.dst_end_time(),
        rule.dst().ut_offset(),
    );
    let mut transitions = [
        (start, Offset::from(rule.dst())),
        (end, Offset::from(rule.std())),
    ];
    transitions.sort_by_key(|(timestamp, _)| *timestamp);
    transitions
}
