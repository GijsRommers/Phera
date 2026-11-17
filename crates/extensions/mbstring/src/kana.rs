//! Kana conversion combines voiced marks: `mb_convert_kana('ｶﾞ', 'KV') === 'ガ'`.
use crate::{codec::decode, encoding, text};
use phera_extension_api::{Context, NativeError, NativeResult, Value, args};
use std::{collections::HashMap, sync::OnceLock};
const FLAGS: &[u8] = b"ARNSKHMCarnskhmcV";
struct Options {
    enabled: [bool; 17],
}
impl Options {
    fn parse(bytes: &[u8]) -> Result<Self, NativeError> {
        let mut enabled = [false; 17];
        for byte in bytes {
            let Some(index) = FLAGS.iter().position(|flag| flag == byte) else {
                return Err(NativeError::value_error(
                    format!(
                        "mb_convert_kana(): Argument #2 ($mode) contains invalid flag: '{}'",
                        char::from(*byte)
                    )
                    .split('\0')
                    .next()
                    .unwrap_or_default(),
                ));
            };
            enabled[index] = true;
            if index == 0 || index == 8 {
                enabled[index + 1] = true;
                enabled[index + 2] = true;
            }
        }
        for index in 0..8 {
            if enabled[index] && enabled[index + 8] {
                let left = if matches!(index, 1 | 2) && enabled[0] {
                    0
                } else {
                    index
                };
                let right = if matches!(index, 1 | 2) && enabled[8] {
                    8
                } else {
                    index + 8
                };
                return Err(conflict(FLAGS[left], FLAGS[right]));
            }
        }
        for (left, right) in [(5, 4), (13, 7), (13, 15), (12, 7), (12, 15)] {
            if enabled[left] && enabled[right] {
                return Err(conflict(FLAGS[left], FLAGS[right]));
            }
        }
        Ok(Self { enabled })
    }
}
fn conflict(left: u8, right: u8) -> NativeError {
    NativeError::value_error(format!(
        "mb_convert_kana(): Argument #2 ($mode) must not combine '{}' and '{}' flags",
        char::from(left),
        char::from(right)
    ))
}
type Mappings = HashMap<(u8, u32, u32), Vec<u32>>;
fn mappings() -> &'static Mappings {
    static TABLE: OnceLock<Mappings> = OnceLock::new();
    TABLE.get_or_init(|| {
        include_str!("../data/kana.txt")
            .lines()
            .filter_map(|line| {
                let mut parts = line.split(' ');
                let flag = *parts.next()?.as_bytes().first()?;
                let key = parts.next()?;
                let (first, second) = key.split_once(':').unwrap_or((key, "0"));
                let value = parts
                    .next()?
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<Vec<u32>, _>>()
                    .ok()?;
                Some(((flag, first.parse().ok()?, second.parse().ok()?), value))
            })
            .collect()
    })
}
pub fn convert(ctx: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let options = Options::parse(args::string(arguments, 1).map_or(b"KV", |s| s.as_bytes()))?;
    let encoding = encoding::argument(ctx, arguments, 2, "mb_convert_kana")?;
    let points = decode(args::bytes(arguments, 0), encoding);
    let mut result = Vec::with_capacity(points.len());
    let mut index = 0;
    while index < points.len() {
        let point = points[index];
        let mut converted = false;
        // Each input character is transformed once, in PHP's option precedence.
        for option in [0, 1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 7, 15, 14] {
            if !options.enabled[option] {
                continue;
            }
            let flag = FLAGS[option];
            if options.enabled[16]
                && matches!(flag, b'K' | b'H')
                && let Some(next) = points.get(index + 1)
                && let Some(mapped) = mappings().get(&(flag, point, *next))
            {
                result.extend(mapped);
                index += 1;
                converted = true;
                break;
            }
            if let Some(mapped) = mappings().get(&(flag, point, 0)) {
                result.extend(mapped);
                converted = true;
                break;
            }
        }
        if !converted {
            result.push(point);
        }
        index += 1;
    }
    Ok(Value::from(text::encode_result(ctx, &result, encoding)))
}
