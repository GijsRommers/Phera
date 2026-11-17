//! PHP's timezone database includes country codes even for historical aliases.
use super::{Call, assoc};
use phera_extension_api::{ArrayKey, NativeError, NativeResult, PhpArray, Value};
use std::sync::LazyLock;

pub const GROUPS: [&str; 11] = ["Africa","America","Antarctica","Arctic","Asia","Atlantic",
    "Australia","Europe","Indian","Pacific","UTC"];

pub struct Location {
    name: &'static str, primary: bool, country: &'static str, latitude: f64, longitude: f64,
    start: usize, count: usize, comments: &'static str,
}
static CATALOG: LazyLock<Vec<Location>> = LazyLock::new(|| {
    include_str!("../../data/locations.tsv").lines().map(|line| {
        let mut f = line.split('\t');
        Location { name: f.next().unwrap(), primary: f.next() == Some("1"),
            country: f.next().unwrap(), latitude: f.next().unwrap().parse::<f64>().unwrap()/1e5-90.,
            longitude: f.next().unwrap().parse::<f64>().unwrap()/1e5-180.,
            start: f.next().unwrap().parse().unwrap(), count: f.next().unwrap().parse().unwrap(),
            comments: f.next().unwrap() }
    }).collect()
});
impl Location {
    pub fn find(name: &str) -> Option<&'static Self> {
        CATALOG.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }
    pub fn transitions(&self) -> impl DoubleEndedIterator<Item=i64> + '_ {
        include_bytes!("../../data/transitions.bin")[self.start..self.start+self.count*8]
            .chunks_exact(8).map(|bytes| i64::from_be_bytes(bytes.try_into().unwrap()))
    }
    pub fn value(&self) -> Value {
        assoc([("country_code",self.country.into()),("latitude",Value::Float(self.latitude)),
            ("longitude",Value::Float(self.longitude)),("comments",self.comments.into())]).into()
    }
}

pub fn identifiers(call: Call<'_>) -> NativeResult {
    let group = phera_extension_api::args::int(call.args,0).unwrap_or(2047);
    let country = call.bytes(1);
    if group == 4096 && country.len() != 2 {
        return Err(NativeError::value_error(format!(
            "{}(): Argument #2 ($countryCode) must be a two-letter ISO 3166-1 compatible country code when argument #1 ($timezoneGroup) is DateTimeZone::PER_COUNTRY", call.site
        )));
    }
    let mut result = PhpArray::new();
    for location in CATALOG.iter() {
        let group_name = location.name.split('/').next().unwrap();
        let matches = match group {
            4096 => location.country.as_bytes() == country,
            2048 => true,
            _ => location.primary && GROUPS.iter().enumerate().any(|(i,name)|
                group & (1<<i) != 0 && name == &group_name),
        };
        if matches {result.push(location.name.into())?;}
    }
    Ok(result.into())
}

type Abbreviation = (&'static str, bool, i64, &'static str);
fn entries(table: &'static str) -> impl Iterator<Item=Abbreviation> {
    table.lines().map(|line| {
        let mut f = line.split('\t');
        (f.next().unwrap(), f.next() == Some("1"), f.next().unwrap().parse().unwrap(), f.next().unwrap())
    })
}

const ABBR: &str = include_str!("../../data/zone-abbreviations.tsv");
pub fn abbreviations(_: Call<'_>) -> NativeResult {
    let mut result = PhpArray::new();
    let mut list = PhpArray::new();
    let mut group = "";
    for (name,dst,offset,id) in entries(ABBR) {
        if name != group && !group.is_empty() {
            result.insert(ArrayKey::str(group), Value::from(std::mem::take(&mut list)));
        }
        group = name;
        list.push(assoc([("dst",Value::Bool(dst)),("offset",Value::Int(offset)),
            ("timezone_id", if id.is_empty() {Value::Null} else {id.into()})]).into())?;
    }
    result.insert(ArrayKey::str(group), list.into());
    Ok(result.into())
}

pub fn reverse(call: Call<'_>) -> NativeResult {
    use phera_extension_api::args;
    let word = call.bytes(0).split(|b| *b==0).next().unwrap();
    if word.eq_ignore_ascii_case(b"utc") || word.eq_ignore_ascii_case(b"gmt") {
        return Ok("UTC".into());
    }
    let offset = args::int(call.args,1).unwrap_or(-1);
    let dst = args::int(call.args,2).unwrap_or(-1);
    let mut matches = entries(ABBR).filter(|r| word.eq_ignore_ascii_case(r.0.as_bytes()));
    let entry = if let Some(first) = matches.next() {
        Some(if offset == -1 || offset == first.2 {first} else {
            matches.find(|r| r.2 == offset).unwrap_or(first)
        })
    } else {
        entries(include_str!("../../data/zone-fallbacks.tsv")).find(|r| r.2 == offset && i64::from(r.1)==dst)
    };
    Ok(entry.filter(|r| !r.3.is_empty()).map_or(Value::Bool(false), |r| r.3.into()))
}
