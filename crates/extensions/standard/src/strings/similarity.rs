//! How alike two strings are: `similar_text`, `levenshtein` and `soundex`,
//! ported from php-src ext/standard/string.c (`php_similar_char`),
//! levenshtein.c and soundex.c.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::{buffer, check_size};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[SIMILAR_TEXT, LEVENSHTEIN, SOUNDEX];

/// `similar_text(string $string1, string $string2, float &$percent = null):
/// int`, the number of bytes the two strings have in common.
///
/// It takes the longest common substring (the first one, if several are
/// equally long), then does the same with what lies left of it in both
/// strings and with what lies right of it. `$percent` is twice the result
/// over the two lengths.
///
/// ```php
/// similar_text("World", "Word", $percent); // 4, and $percent is 88.88…
/// ```
const SIMILAR_TEXT: FunctionEntry = FunctionEntry::sync(
    "similar_text",
    &[
        Param::new("string1", ParamType::String),
        Param::new("string2", ParamType::String),
        Param::new("percent", ParamType::Mixed).by_ref().optional(),
    ],
    similar_text,
);

fn similar_text(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let first = args::bytes(arguments, 0);
    let second = args::bytes(arguments, 1);
    // php_similar_char recurses into both sides; a stack of pairs still to
    // compare does the same without Rust recursion.
    let mut pending = vec![(first, second)];
    let mut common = 0;
    while let Some((left, right)) = pending.pop() {
        let Some(found) = longest_common_substring(left, right) else {
            continue;
        };
        common += found.length;
        let (left_start, right_start) = found.starts;
        // PHP only looks left when the match is not at the start of either
        // string and was not the first candidate it tried.
        if left_start > 0 && right_start > 0 && found.improvements > 1 {
            pending.push((&left[..left_start], &right[..right_start]));
        }
        let (left_end, right_end) = (left_start + found.length, right_start + found.length);
        if left_end < left.len() && right_end < right.len() {
            pending.push((&left[left_end..], &right[right_end..]));
        }
    }
    if let Some(percent) = args::reference(arguments, 2) {
        let total = first.len() + second.len();
        *percent.borrow_mut() = Value::Float(if total == 0 {
            0.0
        } else {
            common as f64 * 200.0 / total as f64
        });
    }
    Ok(Value::Int(common as i64))
}

struct CommonSubstring {
    length: usize,
    /// Where it starts in the left and in the right string.
    starts: (usize, usize),
    /// How often a longer match replaced the best one so far.
    improvements: usize,
}

/// php_similar_str: the first longest common substring, if any.
fn longest_common_substring(left: &[u8], right: &[u8]) -> Option<CommonSubstring> {
    let mut best = CommonSubstring {
        length: 0,
        starts: (0, 0),
        improvements: 0,
    };
    for left_start in 0..left.len() {
        for right_start in 0..right.len() {
            let length = left[left_start..]
                .iter()
                .zip(&right[right_start..])
                .take_while(|(a, b)| a == b)
                .count();
            if length > best.length {
                best.length = length;
                best.starts = (left_start, right_start);
                best.improvements += 1;
            }
        }
    }
    (best.length > 0).then_some(best)
}

/// `levenshtein(string $string1, string $string2, int $insertion_cost = 1,
/// int $replacement_cost = 1, int $deletion_cost = 1): int`, the cheapest
/// way to edit the first string into the second.
///
/// The costs may be negative, and PHP's arithmetic wraps around on overflow.
///
/// ```php
/// levenshtein("kitten", "sitting");          // 3
/// levenshtein("kitten", "sitting", 1, 5, 1); // 5: two replacements cost more than inserting and deleting
/// ```
const LEVENSHTEIN: FunctionEntry = FunctionEntry::sync(
    "levenshtein",
    &[
        Param::new("string1", ParamType::String),
        Param::new("string2", ParamType::String),
        Param::new("insertion_cost", ParamType::Int).optional(),
        Param::new("replacement_cost", ParamType::Int).optional(),
        Param::new("deletion_cost", ParamType::Int).optional(),
    ],
    levenshtein,
);

fn levenshtein(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
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
    // With equal costs the distance is symmetric, and the shorter string
    // makes the shorter rows.
    if left.len() < right.len() && insert == replace && replace == delete {
        std::mem::swap(&mut left, &mut right);
    }
    let columns = right.len() + 1;
    // The two rows of eight-byte numbers count against the string size
    // limit, as PHP's do.
    check_size(columns.saturating_mul(16))?;
    let mut previous = Vec::new();
    let mut current = Vec::new();
    for row in [&mut previous, &mut current] {
        row.try_reserve(columns)
            .map_err(|_| NativeError::error("Could not allocate string"))?;
    }
    previous.extend((0..columns).map(|column| (column as i64).wrapping_mul(insert)));
    current.resize(columns, 0);
    for &byte in left {
        current[0] = previous[0].wrapping_add(delete);
        for (column, &other) in right.iter().enumerate() {
            let replaced = previous[column].wrapping_add(if byte == other { 0 } else { replace });
            let deleted = previous[column + 1].wrapping_add(delete);
            let inserted = current[column].wrapping_add(insert);
            current[column + 1] = replaced.min(deleted).min(inserted);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    Ok(Value::Int(previous[columns - 1]))
}

/// `soundex(string $string): string`, a four-character code for how an
/// English name sounds: its first letter and three digits.
///
/// Bytes that are no letters are skipped. Letters with the same digit next
/// to each other count once; a letter without one (a vowel, H, W or Y) in
/// between separates them.
///
/// ```php
/// soundex("Robert"); // "R163"
/// soundex("Rupert"); // "R163"
/// ```
const SOUNDEX: FunctionEntry = FunctionEntry::sync(
    "soundex",
    &[Param::new("string", ParamType::String)],
    soundex,
);

fn soundex(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    /// The digit of each letter from A to Z; 0 is a vowel or ignored letter.
    const CODES: &[u8; 26] = b"01230120022455012623010202";
    let mut result = buffer(4)?;
    let mut previous = 0;
    let letters = args::bytes(arguments, 0)
        .iter()
        .map(u8::to_ascii_uppercase)
        .filter(u8::is_ascii_uppercase);
    for letter in letters {
        let code = CODES[usize::from(letter - b'A')];
        if result.is_empty() {
            result.push(letter);
        } else if code != previous && code != b'0' {
            result.push(code);
        }
        previous = code;
        if result.len() == 4 {
            break;
        }
    }
    result.resize(4, b'0');
    Ok(Value::from(result))
}
