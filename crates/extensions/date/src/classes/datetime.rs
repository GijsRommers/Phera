//! `modify("+1 month")` edits a DateTime in place and returns a new DateTimeImmutable.
use super::{Call, make, state, store, zone_kind::ZoneKind};
use crate::{calendar::Civil, format::DateFormat, parse::{self, state::State},
    parse_format::{self, result::Parsed}, zone};
use phera_extension_api::{NativeError, NativeResult, Value, args};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct Instant {pub timestamp:i64, pub micros:i64, pub zone:ZoneKind}
impl Instant {
    pub fn format(&self) -> DateFormat {self.zone.format(self.timestamp,self.micros)}
    pub fn wall(&mut self,civil:Civil, micros:i64, daylight: Option<bool>) {
        let adjusted = Civil { second:civil.second.wrapping_add(micros.div_euclid(1_000_000)), ..civil };
        self.timestamp = self.zone.resolve(adjusted,daylight);
        self.micros = micros.rem_euclid(1_000_000);
    }
    pub fn apply(&mut self, parsed: &mut State, base: Civil, daylight:Option<bool>) {
        let micros = parsed.parsed.microsecond.unwrap_or(0);
        let civil = parse::apply::wall(parsed,base);
        self.timestamp = self.zone.resolve(civil,daylight);
        self.micros = micros.wrapping_add(parsed.delta[6]).rem_euclid(1_000_000);
    }
    pub fn changed(self, call: Call<'_>) -> NativeResult {
        let obj = super::receiver(call.this)?;
        if obj.class.is_instance_of("DateTimeImmutable") {
            let copy = call.ctx.create_object(obj.class.name(), obj.to_array())?;
            if let Value::Object(o) = &copy {store(o,self);}
            Ok(copy)
        } else {
            store(obj,self);
            Ok(Value::Object(obj.clone()))
        }
    }
}
#[derive(Default)]
struct LastErrors(Option<Value>);
fn remember(call: &mut Call<'_>, parsed: &Parsed) {
    let errors = (!parsed.errors.is_empty() || !parsed.warnings.is_empty())
        .then(|| Value::from(parsed.diagnostics()));
    call.ctx.state().get_or_default::<LastErrors>().0 = errors;
}
pub fn last_errors(call: Call<'_>) -> NativeResult {
    Ok(call.ctx.state().get_or_default::<LastErrors>().0.clone().unwrap_or(Value::Bool(false)))
}

pub fn diagnostic(input: &[u8], error: &(usize,String), prefix:&str) -> String {
    let trimmed = input.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(0);
    let character = input.get(trimmed + error.0).copied().filter(|b| *b != 0).unwrap_or(b' ');
    let shown = String::from_utf8_lossy(input.split(|b| *b==0).next().unwrap());
    format!("{prefix} ({shown}) at position {} ({}): {}",error.0,String::from_utf8_lossy(&[character]),error.1)
}
pub fn construct(mut call: Call<'_>) -> NativeResult { initialize(&mut call,false) }
pub fn from_format(mut call: Call<'_>) -> NativeResult { initialize(&mut call,true) }
fn initialize(call: &mut Call<'_>, formatted: bool) -> NativeResult {
    let index = usize::from(formatted);
    let input = args::bytes(call.args,index);
    let mut parsed = if formatted {
        if input.contains(&0) {
            return Err(NativeError::value_error(format!(
                "{}(): Argument #2 ($datetime) must not contain any null bytes", call.site)));
        }
        let p = parse_format::parse(call.bytes(0),input);
        State { weekday:p.weekday, behavior:parse::token::WeekdayBehavior::OnOrAfter,
            parsed:p, ..State::default() }
    } else {parse::parse(if input.is_empty() {b"now"} else {input})};
    remember(call,&parsed.parsed);
    if let Some(error) = parsed.parsed.errors.first() {
        if !formatted && call.method() {
            return Err(NativeError::throw("DateMalformedStringException",
                diagnostic(input,error,"Failed to parse time string")));
        }
        return Ok(Value::Bool(false));
    }
    let explicit = parsed.parsed.zone.as_ref().and_then(ZoneKind::parsed);
    let base_zone = if let Some(obj) = args::object(call.args,index+1) {state(obj,"DateTimeZone")?}
        else if let Some(z @ ZoneKind::Identifier(_)) = &explicit { z.clone() }
        else {ZoneKind::Identifier(zone::current(call.ctx))};
    let clock = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let now = base_zone.format(clock.as_secs() as i64,i64::from(clock.subsec_micros()));
    let has_fields = parsed.parsed.fields.iter().any(Option::is_some);
    parsed.parsed.microsecond.get_or_insert(if has_fields {0} else {i64::from(now.microsecond)});
    let daylight = matches!(explicit,Some(ZoneKind::Identifier(_))).then_some(true);
    let mut date = Instant { timestamp:0, micros:0, zone:explicit.unwrap_or(base_zone) };
    date.apply(&mut parsed,now.civil,daylight);
    call.result(date)
}

pub fn from_timestamp(mut call: Call<'_>) -> NativeResult {
    let (timestamp,micros) = match call.args[0] {
        Value::Int(n) => (n,0),
        Value::Float(f) if f >= i64::MIN as f64 && f < -(i64::MIN as f64) => {
            let fraction = (f.fract()*1e6).round() as i64;
            ((f as i64).wrapping_add(fraction.div_euclid(1_000_000)),fraction.rem_euclid(1_000_000))
        }
        Value::Float(f) => {
            let shown = phera_extension_api::value::float_repr(f).replace("E","e");
            return Err(NativeError::throw("DateRangeError",format!(
                "{}(): Argument #1 ($timestamp) must be a finite number between {} and {}.999999, {shown} given",
                call.site,i64::MIN,i64::MAX)));
        }
        _ => unreachable!("prepared numeric parameter")
    };
    call.result(Instant { timestamp,micros,zone:ZoneKind::Fixed(0) })
}
pub fn from_object(mut call: Call<'_>) -> NativeResult {
    let date: Instant = state(args::object(call.args,0).unwrap(),call.class)?;
    call.result(date)
}
pub fn format(call: Call<'_>) -> NativeResult {
    Ok(call.native::<Instant>()?.format().render(call.bytes(0)).into())
}
pub fn timestamp(call: Call<'_>) -> NativeResult {Ok(Value::Int(call.native::<Instant>()?.timestamp))}
pub fn micros(call: Call<'_>) -> NativeResult {Ok(Value::Int(call.native::<Instant>()?.micros))}
pub fn offset(call: Call<'_>) -> NativeResult {
    let d:Instant=call.native()?; Ok(Value::Int(i64::from(d.zone.offset(d.timestamp).seconds)))
}
pub fn timezone(call: Call<'_>) -> NativeResult {
    let d:Instant=call.native()?; make(call.ctx,"DateTimeZone",d.zone)
}
pub fn modify(mut call: Call<'_>) -> NativeResult {
    let mut date:Instant = call.native()?;
    let input = args::bytes(call.args,0);
    let mut p = parse::parse(input);
    remember(&mut call,&p.parsed);
    if let Some(error) = p.parsed.errors.first() {
        let message = format!("{}(): {}",call.site, diagnostic(input,error,"Failed to parse time string"));
        if call.method() {return Err(NativeError::throw("DateMalformedStringException",message));}
        call.ctx.warning(message);
        return Ok(Value::Bool(false));
    }
    let base=date.format();
    if p.have_zone>0 && matches!(p.parsed.zone,Some(parse_format::result::ParsedZone::Offset(0)))
        && p.parsed.fields == [1970,1,1,0,0,0].map(Some) && p.parsed.microsecond==Some(0) {
        date.zone=ZoneKind::Fixed(0);
    }
    p.have_date=false;
    p.parsed.microsecond.get_or_insert(date.micros);
    date.apply(&mut p,base.civil,Some(base.offset.daylight));
    date.changed(call)
}

pub fn set_zone(call: Call<'_>) -> NativeResult {
    let mut date:Instant = call.native()?;
    date.zone=state(args::object(call.args,0).unwrap(),"DateTimeZone")?;
    date.changed(call)
}
pub fn set_timestamp(call: Call<'_>) -> NativeResult {
    let mut date:Instant=call.native()?;
    date.timestamp=call.int(0);
    date.micros=0;
    date.changed(call)
}
pub fn set_micros(call: Call<'_>) -> NativeResult {
    let n=call.int(0);
    if !(0..1_000_000).contains(&n) {
        return Err(NativeError::throw("DateRangeError",format!(
            "{}(): Argument #1 ($microsecond) must be between 0 and 999999, {n} given",call.site)));
    }
    let mut date:Instant=call.native()?; date.micros=n; date.changed(call)
}
pub fn set_time(call: Call<'_>) -> NativeResult {
    let mut date:Instant=call.native()?;
    let base=date.format();
    date.wall(Civil { hour:call.int(0), minute:call.int(1), second:call.int(2), ..base.civil },
        call.int(3),Some(base.offset.daylight));
    date.changed(call)
}
pub fn set_date(call: Call<'_>) -> NativeResult {
    let mut date:Instant=call.native()?;
    let base=date.format();
    date.wall(Civil { year:call.int(0), month:call.int(1), day:call.int(2), ..base.civil },
        date.micros,Some(base.offset.daylight));
    date.changed(call)
}
pub fn set_iso(call: Call<'_>) -> NativeResult {
    let mut date:Instant=call.native()?;
    let base=date.format();
    let jan4=Civil {year:call.int(0),month:1,day:4,..base.civil};
    let day = i128::from(4-(jan4.weekday()+6)%7) + (i128::from(call.int(1))-1)*7
        + i128::from(args::int(call.args,2).unwrap_or(1))-1;
    date.wall(Civil {day:day as i64,..jan4},date.micros,Some(base.offset.daylight));
    date.changed(call)
}
