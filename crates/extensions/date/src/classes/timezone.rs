//! `(new DateTimeZone("+02:00"))->getLocation()` returns false for a fixed offset.
use super::{Call, assoc, catalog::Location, datetime::Instant, state, zone_kind::ZoneKind};
use crate::{calendar::Civil, format::DateFormat, zone::Zone};
use phera_extension_api::{NativeError, NativeResult, PhpArray, Value, args};

pub fn parse(input: &[u8]) -> Result<ZoneKind,String> {
    let mut end = 0;
    let (parsed,found) = crate::parse_format::zone::parse(input, &mut end);
    if let Some(zone) = ZoneKind::parsed(&parsed) {
        if matches!(zone, ZoneKind::Fixed(n) if n.unsigned_abs() >= 360000) {
            return Err(format!("Timezone offset is out of range ({})",String::from_utf8_lossy(input)));
        }
        if found && end == input.len() {return Ok(zone);}
    }
    Err(format!("Unknown or bad timezone ({})",String::from_utf8_lossy(input)))
}

pub fn construct(mut call: Call<'_>) -> NativeResult {
    if call.bytes(0).contains(&0) {
        return Err(NativeError::value_error(format!(
            "{}(): Argument #1 ($timezone) must not contain any null bytes",call.site)));
    }
    match parse(call.bytes(0)) {
        Ok(zone) => call.result(zone),
        Err(reason) => {
            let message = format!("{}(): {reason}",call.site);
            if call.method() {return Err(NativeError::throw("DateInvalidTimeZoneException",message));}
            call.ctx.warning(message);
            Ok(Value::Bool(false))
        }
    }
}

pub fn name(call: Call<'_>) -> NativeResult {Ok(call.native::<ZoneKind>()?.name().into())}
pub fn offset(call: Call<'_>) -> NativeResult {
    let date: Instant = state(args::object(call.args,0).unwrap(),"DateTime")?;
    Ok(Value::Int(i64::from(call.native::<ZoneKind>()?.offset(date.timestamp).seconds)))
}

pub fn location(call: Call<'_>) -> NativeResult {
    let ZoneKind::Identifier(zone) = call.native()? else {return Ok(Value::Bool(false));};
    Ok(Location::find(&zone.name).map_or(Value::Bool(false),Location::value))
}

pub fn transitions(call: Call<'_>) -> NativeResult {
    let ZoneKind::Identifier(zone) = call.native()? else {return Ok(Value::Bool(false));};
    let begin = args::int(call.args,0).unwrap_or(i64::MIN);
    let end = args::int(call.args,1).unwrap_or(i64::from(i32::MAX));
    let mut result = PhpArray::new();
    let mut append = |ts: i64| -> Result<(),NativeError> {
        let offset = zone.offset(ts);
        let time = DateFormat::new(ts,&Zone::default(),false).render(b"x-m-d\\TH:i:sP");
        result.push(assoc([("ts",Value::Int(ts)),("time",time.into()),
            ("offset",Value::Int(i64::from(offset.seconds))),("isdst",Value::Bool(offset.daylight)),
            ("abbr",offset.abbreviation.into())]).into())?;
        Ok(())
    };
    append(begin)?;
    if let Some(location) = Location::find(&zone.name) {
        for ts in location.transitions().filter(|t| *t > begin && *t < end) {append(ts)?;}
        if let Some(last) = location.transitions().next_back()
            && zone.alternate_rule().is_some() {
            let start = Civil::from_timestamp(begin.max(last)).year;
            for year in start..=Civil::from_timestamp(end).year {
                for ts in zone.year_transitions(year) {
                    if ts > last && ts >= begin && ts <= end {append(ts)?;}
                }
            }
        }
    }
    Ok(result.into())
}
