//! `htmlentities('fjord & é', ENT_HTML5)` returns `"&fjlig;ord &amp; &eacute;"`.
//! Named HTML5 entities may cover two consecutive Unicode characters.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Value};

use super::{Decoded, ENCODE, Encoding, Options, SubstituteEntities, encode, entities::Entity};

pub(super) const HTMLENTITIES: FunctionEntry =
    FunctionEntry::sync("htmlentities", ENCODE, htmlentities);

fn htmlentities(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    encode(context, arguments, SubstituteEntities::All)
}

pub(super) fn entry(
    rows: &[Entity],
    encoding: &Encoding,
    options: Options,
    code: u32,
    rest: &[u8],
) -> Option<(&'static str, usize)> {
    if u8::try_from(code).is_ok_and(|byte| !options.quotes.includes(byte)) {
        return None;
    }
    let code = unicode(encoding, code);
    let start = rows.partition_point(|row| row.first < code);
    let next = (!rest.is_empty()).then(|| encoding.next(rest));
    let mut single = None;
    for row in rows[start..].iter().take_while(|row| row.first == code) {
        match row.second {
            None => single = Some((row.name, 0)),
            Some(second) => {
                if let Some(Decoded::Valid { code, length }) = next
                    && unicode(encoding, code) == second
                {
                    return Some((row.name, length));
                }
            }
        }
    }
    single
}

pub(super) fn unicode(encoding: &Encoding, code: u32) -> u32 {
    match encoding {
        Encoding::SingleByte(legacy) => legacy.to_unicode(code as u8),
        _ => code,
    }
}
