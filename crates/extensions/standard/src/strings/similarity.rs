//! Similarity algorithms retain PHP's tie ordering and integer overflow behavior.
use super::{buffer, bytes};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};
const SIMILAR: &[Param] = &[
    Param::new("string1", ParamType::String),
    Param::new("string2", ParamType::String),
    Param::new("percent", ParamType::Mixed).by_ref().optional(),
];
const DISTANCE: &[Param] = &[
    Param::new("string1", ParamType::String),
    Param::new("string2", ParamType::String),
    Param::new("insertion_cost", ParamType::Int).optional(),
    Param::new("replacement_cost", ParamType::Int).optional(),
    Param::new("deletion_cost", ParamType::Int).optional(),
];
const STRING: &[Param] = &[Param::new("string", ParamType::String)];
pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("similar_text", SIMILAR, similar));
    registry.function(FunctionEntry::sync("levenshtein", DISTANCE, distance));
    registry.function(FunctionEntry::sync("soundex", STRING, soundex));
}
/// An explicit work stack reproduces PHP's recursive longest-match partitioning.
fn similar(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let first = args::bytes(arguments, 0);
    let second = args::bytes(arguments, 1);
    let mut pending = vec![(first, second)];
    let mut sum = 0usize;
    while let Some((left, right)) = pending.pop() {
        let mut best = 0;
        let mut updates = 0;
        let mut position = (0, 0);
        for index in 0..left.len() {
            for other in 0..right.len() {
                let length = left
                    .iter()
                    .skip(index)
                    .zip(right.iter().skip(other))
                    .take_while(|(a, b)| a == b)
                    .count();
                if length > best {
                    best = length;
                    position = (index, other);
                    updates += 1;
                }
            }
        }
        if best == 0 {
            continue;
        }
        sum += best;
        let (left_pos, right_pos) = position;
        if left_pos > 0 && right_pos > 0 && updates > 1 {
            pending.push((
                left.get(..left_pos).unwrap_or_default(),
                right.get(..right_pos).unwrap_or_default(),
            ));
        }
        if left_pos + best < left.len() && right_pos + best < right.len() {
            pending.push((
                left.get(left_pos + best..).unwrap_or_default(),
                right.get(right_pos + best..).unwrap_or_default(),
            ));
        }
    }
    if let Some(reference) = args::reference(arguments, 2) {
        let total = first.len().saturating_add(second.len());
        let percent = if total == 0 {
            0.0
        } else {
            sum as f64 * 200.0 / total as f64
        };
        *reference
            .try_borrow_mut()
            .map_err(|_| NativeError::error("Cannot modify borrowed reference"))? =
            Value::Float(percent);
    }
    Ok(Value::Int(sum as i64))
}
/// Custom costs may be negative; PHP's signed machine arithmetic wraps on overflow.
fn distance(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut left = args::bytes(arguments, 0);
    let mut right = args::bytes(arguments, 1);
    let insert = args::int(arguments, 2).unwrap_or(1);
    let replace = args::int(arguments, 3).unwrap_or(1);
    let delete = args::int(arguments, 4).unwrap_or(1);
    if left.is_empty() {
        return Ok(Value::Int((right.len() as i64).wrapping_mul(insert)));
    }
    if right.is_empty() {
        return Ok(Value::Int((left.len() as i64).wrapping_mul(delete)));
    }
    if left.len() < right.len() && insert == replace && replace == delete {
        std::mem::swap(&mut left, &mut right);
    }
    let length = right.len().saturating_add(1);
    let allocation = buffer(length.saturating_mul(16))?;
    drop(allocation);
    let mut previous = Vec::new();
    let mut next = Vec::new();
    previous
        .try_reserve(length)
        .map_err(|_| NativeError::error("Could not allocate string"))?;
    next.try_reserve(length)
        .map_err(|_| NativeError::error("Could not allocate string"))?;
    previous.extend((0..length).map(|index| (index as i64).wrapping_mul(insert)));
    next.resize(length, 0);
    for &byte in left {
        if let Some(first) = next.first_mut() {
            *first = previous
                .first()
                .copied()
                .unwrap_or_default()
                .wrapping_add(delete);
        }
        for (index, &other) in right.iter().enumerate() {
            let diagonal = previous
                .get(index)
                .copied()
                .unwrap_or_default()
                .wrapping_add(if byte == other { 0 } else { replace });
            let above = previous
                .get(index + 1)
                .copied()
                .unwrap_or_default()
                .wrapping_add(delete);
            let beside = next
                .get(index)
                .copied()
                .unwrap_or_default()
                .wrapping_add(insert);
            if let Some(target) = next.get_mut(index + 1) {
                *target = diagonal.min(above).min(beside);
            }
        }
        std::mem::swap(&mut previous, &mut next);
    }
    Ok(Value::Int(previous.last().copied().unwrap_or_default()))
}
/// Vowels reset duplicate suppression; nonletters do not reset it.
fn soundex(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    const CODES: &[u8] = b"01230120022455012623010202";
    let mut result = buffer(4)?;
    let mut previous = 0;
    for byte in args::bytes(arguments, 0)
        .iter()
        .map(u8::to_ascii_uppercase)
        .filter(u8::is_ascii_uppercase)
    {
        let code = CODES.get(usize::from(byte - b'A')).copied().unwrap_or(b'0');
        if result.is_empty() {
            result.push(byte);
        } else if code != previous && code != b'0' {
            result.push(code);
        }
        previous = code;
        if result.len() == 4 {
            break;
        }
    }
    result.resize(4, b'0');
    Ok(bytes(result))
}
