//! Converting between half-width and full-width forms, mostly for Japanese:
//!
//! ```php
//! mb_convert_kana('ｶﾞ', 'KV');   // "ガ": half-width katakana and its voiced mark joined
//! mb_convert_kana('ＡＢ', 'a');  // "AB": full-width letters to ASCII
//! mb_convert_kana('か', 'C');    // "カ": hiragana to katakana
//! ```
//!
//! Each flag is one letter (`"KV"` is the default). The mappings were
//! generated from PHP by `examples/generate_kana.rs` into `data/kana.txt`.

use std::collections::HashMap;
use std::sync::OnceLock;

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::codec::decode;
use crate::text::encode_result;
use crate::{ENCODING, STRING, encoding};

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "mb_convert_kana",
        MB_CONVERT_KANA,
        mb_convert_kana,
    ));
}

/// Every flag PHP accepts. Upper case converts towards full width, lower
/// case towards half width; `V` joins voiced sound marks.
const FLAGS: &[u8; 17] = b"ARNSKHMCarnskhmcV";

/// The order in which PHP tries the flags on each character; the first
/// that maps it wins.
const PRECEDENCE: &[u8] = b"ARNSKHMarnskhCcm";

/// Flags that cannot be combined, besides a letter with its other case.
const CONFLICTS: [(u8, u8); 5] = [
    (b'H', b'K'),
    (b'h', b'C'),
    (b'h', b'c'),
    (b'k', b'C'),
    (b'k', b'c'),
];

struct Mode {
    enabled: [bool; 17],
}

impl Mode {
    fn has(&self, flag: u8) -> bool {
        FLAGS
            .iter()
            .position(|&known| known == flag)
            .is_some_and(|index| self.enabled[index])
    }

    fn parse(flags: &[u8]) -> Result<Self, NativeError> {
        let mut mode = Mode {
            enabled: [false; 17],
        };
        for &flag in flags {
            let Some(index) = FLAGS.iter().position(|&known| known == flag) else {
                let message = format!(
                    "mb_convert_kana(): Argument #2 ($mode) contains invalid flag: '{}'",
                    char::from(flag)
                );
                // PHP's message is a C string: it ends at a NUL flag.
                let message = message.split('\0').next().unwrap_or_default();
                return Err(NativeError::value_error(message));
            };
            mode.enabled[index] = true;
            // `A` includes `R` and `N`, and `a` includes `r` and `n`.
            if flag == b'A' || flag == b'a' {
                mode.enabled[index + 1] = true;
                mode.enabled[index + 2] = true;
            }
        }
        for (&upper, &lower) in FLAGS[..8].iter().zip(&FLAGS[8..16]) {
            if mode.has(upper) && mode.has(lower) {
                // PHP names `A` or `a` when `R` or `N` came from it.
                let implied = matches!(upper, b'R' | b'N');
                let left = if implied && mode.has(b'A') {
                    b'A'
                } else {
                    upper
                };
                let right = if implied && mode.has(b'a') {
                    b'a'
                } else {
                    lower
                };
                return Err(conflict(left, right));
            }
        }
        for (left, right) in CONFLICTS {
            if mode.has(left) && mode.has(right) {
                return Err(conflict(left, right));
            }
        }
        Ok(mode)
    }
}

fn conflict(left: u8, right: u8) -> NativeError {
    NativeError::value_error(format!(
        "mb_convert_kana(): Argument #2 ($mode) must not combine '{}' and '{}' flags",
        char::from(left),
        char::from(right)
    ))
}

/// Keyed by flag, code point and the following code point, which is 0 for
/// a mapping of one character and a voiced sound mark for `KV` and `HV`.
type Mappings = HashMap<(u8, u32, u32), Vec<u32>>;

/// `data/kana.txt` has one mapping per line: the flag, the code point (or
/// two, `point:next`) and the comma-separated result.
fn mappings() -> &'static Mappings {
    static TABLE: OnceLock<Mappings> = OnceLock::new();
    TABLE.get_or_init(|| {
        include_str!("../data/kana.txt")
            .lines()
            .filter_map(|line| {
                let mut parts = line.split(' ');
                let flag = *parts.next()?.as_bytes().first()?;
                let key = parts.next()?;
                let (point, next) = key.split_once(':').unwrap_or((key, "0"));
                let mapped = parts
                    .next()?
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<Vec<u32>, _>>()
                    .ok()?;
                Some(((flag, point.parse().ok()?, next.parse().ok()?), mapped))
            })
            .collect()
    })
}

const MB_CONVERT_KANA: &[Param] = &[
    STRING,
    Param::new("mode", ParamType::String).optional(),
    ENCODING,
];

/// `mb_convert_kana(string $string, string $mode = "KV", ?string $encoding = null): string`
fn mb_convert_kana(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mode = Mode::parse(args::string(a, 1).map_or(b"KV", |mode| mode.as_bytes()))?;
    let encoding = encoding::argument(ctx, a, 2, "mb_convert_kana")?;
    let points = decode(args::bytes(a, 0), encoding);
    let joins_voiced = mode.has(b'V');
    let mut converted = Vec::with_capacity(points.len());
    let mut index = 0;
    while index < points.len() {
        let point = points[index];
        let next = points.get(index + 1).copied();
        let mapping = PRECEDENCE
            .iter()
            .filter(|&&flag| mode.has(flag))
            .find_map(|&flag| {
                if joins_voiced
                    && matches!(flag, b'K' | b'H')
                    && let Some(next) = next
                    && let Some(joined) = mappings().get(&(flag, point, next))
                {
                    return Some((joined, 2));
                }
                mappings().get(&(flag, point, 0)).map(|mapped| (mapped, 1))
            });
        match mapping {
            Some((mapped, consumed)) => {
                converted.extend(mapped);
                index += consumed;
            }
            None => {
                converted.push(point);
                index += 1;
            }
        }
    }
    Ok(Value::from(encode_result(ctx, &converted, encoding)))
}
