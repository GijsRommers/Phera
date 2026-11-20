//! English CLDR spellout and ordinal rules used by NumberFormatter.
//!
//! ```php
//! (new NumberFormatter('en_US', NumberFormatter::SPELLOUT))->format(21); // "twenty-one"
//! (new NumberFormatter('en_US', NumberFormatter::ORDINAL))->format(21); // "21st"
//! ```

use fixed_decimal::{Decimal, Sign, SignedRoundingMode, UnsignedRoundingMode};

use super::{Formatter, Style};

const CARDINALS: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const ORDINALS: [&str; 20] = [
    "zeroth",
    "first",
    "second",
    "third",
    "fourth",
    "fifth",
    "sixth",
    "seventh",
    "eighth",
    "ninth",
    "tenth",
    "eleventh",
    "twelfth",
    "thirteenth",
    "fourteenth",
    "fifteenth",
    "sixteenth",
    "seventeenth",
    "eighteenth",
    "nineteenth",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const TENTHS: [&str; 10] = [
    "",
    "",
    "twentieth",
    "thirtieth",
    "fortieth",
    "fiftieth",
    "sixtieth",
    "seventieth",
    "eightieth",
    "ninetieth",
];
const SCALES: [(u64, &str); 6] = [
    (1_000_000_000_000_000, "quadrillion"),
    (1_000_000_000_000, "trillion"),
    (1_000_000_000, "billion"),
    (1_000_000, "million"),
    (1_000, "thousand"),
    (100, "hundred"),
];
const NUMBERING: &str = "%spellout-numbering";
const CARDINAL: &str = "%spellout-cardinal";
const SPELLED_ORDINAL: &str = "%spellout-ordinal";
const DIGITS_ORDINAL: &str = "%digits-ordinal";
const HALF_EVEN: SignedRoundingMode = SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfEven);

pub(super) fn accepts_rule(style: Style, rule: &str) -> bool {
    match style {
        Style::Spellout => matches!(rule, NUMBERING | CARDINAL | SPELLED_ORDINAL | ""),
        Style::Ordinal => matches!(rule, DIGITS_ORDINAL | ""),
        _ => false,
    }
}

pub(super) fn default_rule(style: Style) -> Option<&'static str> {
    match style {
        Style::Spellout => Some(NUMBERING),
        Style::Ordinal => Some(DIGITS_ORDINAL),
        _ => None,
    }
}

fn rule(state: &Formatter) -> &str {
    state.text_attributes.get(&6).map_or_else(
        || default_rule(state.style).unwrap_or_default(),
        String::as_str,
    )
}

pub(super) fn format(state: &Formatter, value: &Decimal) -> Option<String> {
    let positive = value.clone().with_sign(Sign::None);
    let text = positive.to_string();
    let (integer, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let integer = integer.parse::<u64>().ok();
    let negative = value.sign() == Sign::Negative;
    if negative && integer == Some(i64::MIN.unsigned_abs()) {
        return Some(state.decimal.format_to_string(value));
    }
    if state.style == Style::Ordinal {
        let rounded = positive.rounded_with_mode(0, HALF_EVEN);
        let mut output = String::new();
        if negative {
            output.push('−');
        }
        output.push_str(&state.decimal.format_to_string(&rounded));
        output.push_str(suffix(integer.unwrap_or_default()));
        return Some(output);
    }
    let ordinal = rule(state) == SPELLED_ORDINAL;
    let fraction = fraction.trim_end_matches('0');
    let mut output = String::new();
    if negative && (integer != Some(0) || !fraction.is_empty()) {
        output.push_str("minus ");
    }
    if ordinal && !fraction.is_empty() {
        let mut rounded = positive.rounded_with_mode(-1, HALF_EVEN);
        rounded.absolute.trim_end();
        output.push_str(&state.ungrouped.format_to_string(&rounded));
    } else if let Some(integer) = integer.filter(|n| *n < 1_000_000_000_000_000_000) {
        words(integer, ordinal, &mut output);
        if !fraction.is_empty() {
            output.push_str(" point");
            for digit in fraction.bytes() {
                output.push(' ');
                output.push_str(CARDINALS.get(usize::from(digit.checked_sub(b'0')?))?);
            }
        }
    } else {
        return Some(state.decimal.format_to_string(value));
    }
    Some(output)
}

pub(super) fn nonfinite(state: &Formatter, value: f64) -> String {
    let rule = rule(state);
    if value.is_nan() {
        return if rule == SPELLED_ORDINAL {
            "NaN"
        } else {
            "not a number"
        }
        .to_owned();
    }
    let text = match rule {
        CARDINAL => "infinite",
        SPELLED_ORDINAL => "infinitieth",
        _ => "infinity",
    };
    if value.is_sign_negative() {
        format!("minus {text}")
    } else {
        text.to_owned()
    }
}

fn suffix(value: u64) -> &'static str {
    match (value % 100, value % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    }
}

fn words(mut value: u64, ordinal: bool, output: &mut String) {
    for (divisor, name) in SCALES {
        if value < divisor {
            continue;
        }
        // Each quotient is below the first divisor and strictly smaller than the input.
        words(value / divisor, false, output);
        output.push(' ');
        output.push_str(name);
        value %= divisor;
        if value == 0 {
            if ordinal {
                output.push_str("th");
            }
            return;
        }
        output.push(' ');
    }
    let value = value as usize;
    if value < 20 {
        output.push_str(if ordinal {
            ORDINALS[value]
        } else {
            CARDINALS[value]
        });
        return;
    }
    let (tens, ones) = (value / 10, value % 10);
    if ordinal && ones == 0 {
        output.push_str(TENTHS[tens]);
    } else {
        output.push_str(TENS[tens]);
        if ones != 0 {
            output.push('-');
            output.push_str(if ordinal {
                ORDINALS[ones]
            } else {
                CARDINALS[ones]
            });
        }
    }
}
