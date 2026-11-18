//! `timezone_name_from_abbr("EST", -18000)` uses timelib’s ordered lookup.

use phera_extension_api::{ArrayKey, NativeResult, PhpArray, Value};

use super::assoc;

type Abbreviation = (&'static str, bool, i64, &'static str);
fn entries(table: &'static str) -> impl Iterator<Item = Abbreviation> {
    table.lines().map(|line| {
        let mut f = line.split('\t');
        (
            f.next().unwrap(),
            f.next() == Some("1"),
            f.next().unwrap().parse().unwrap(),
            f.next().unwrap_or_default(),
        )
    })
}

const ABBR: &str = include_str!("../../data/zone-abbreviations.tsv");
pub fn abbreviations() -> NativeResult {
    let mut result = PhpArray::new();
    let mut list = PhpArray::new();
    let mut group = "";
    for (name, dst, offset, id) in entries(ABBR) {
        if name != group && !group.is_empty() {
            result.insert(ArrayKey::str(group), Value::from(std::mem::take(&mut list)));
        }
        group = name;
        list.push(
            assoc([
                ("dst", Value::Bool(dst)),
                ("offset", Value::Int(offset)),
                (
                    "timezone_id",
                    if id.is_empty() {
                        Value::Null
                    } else {
                        id.into()
                    },
                ),
            ])
            .into(),
        )?;
    }
    result.insert(ArrayKey::str(group), list.into());
    Ok(result.into())
}

pub fn reverse(arguments: &[Value]) -> NativeResult {
    use phera_extension_api::args;
    let word = args::bytes(arguments, 0).split(|b| *b == 0).next().unwrap();
    if word.eq_ignore_ascii_case(b"utc") || word.eq_ignore_ascii_case(b"gmt") {
        return Ok("UTC".into());
    }
    let offset = args::int(arguments, 1).unwrap_or(-1);
    let dst = args::int(arguments, 2).unwrap_or(-1) as i32;
    let mut matches = entries(ABBR).filter(|r| word.eq_ignore_ascii_case(r.0.as_bytes()));
    let entry = if let Some(first) = matches.next() {
        Some(if offset == -1 || offset == first.2 {
            first
        } else {
            matches.find(|r| r.2 == offset).unwrap_or(first)
        })
    } else {
        entries(include_str!("../../data/zone-fallbacks.tsv"))
            .find(|r| r.2 == offset && i32::from(r.1) == dst)
    };
    Ok(entry
        .filter(|r| !r.3.is_empty())
        .map_or(Value::Bool(false), |r| r.3.into()))
}
