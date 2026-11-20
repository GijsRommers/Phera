//! Attributes change the rounding and appearance of subsequent formats.
//!
//! ```php
//! $formatter->setAttribute(NumberFormatter::FRACTION_DIGITS, 2);
//! $formatter->format(1); // "1.00" in en_US
//! ```

use phera_extension_api::{Context, NativeResult, Value, args};

use super::{Call, Formatter, Operation, Style, affixes, render, spellout};

#[derive(Clone)]
pub struct Attributes {
    pub parse_integer: bool,
    pub grouping: bool,
    pub decimal_always: bool,
    pub integer_max: i64,
    pub integer_min: i16,
    pub fraction_max: i16,
    pub fraction_min: i16,
    pub custom_fraction: bool,
    pub multiplier: i64,
    pub group_size: i16,
    pub second_group_size: i16,
    pub custom_group_size: bool,
    pub rounding: i64,
    pub increment: f64,
    pub width: Option<i16>,
    pub pad_position: i64,
    pub significant: bool,
    pub significant_min: Option<i16>,
    pub significant_max: Option<i16>,
    pub lenient: bool,
}

impl Attributes {
    pub fn new(style: Style, pattern: &str) -> Self {
        let positive = pattern.split(';').next().unwrap_or_default();
        let digits = numeric_pattern(positive);
        let (integer, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        let mut groups = integer
            .rsplit(',')
            .map(|group| group.chars().count() as i16);
        let primary = groups.next().unwrap_or(0);
        let _left = groups.next();
        let secondary = if groups.next().is_some() {
            integer
                .rsplit(',')
                .nth(1)
                .map_or(0, |part| part.len() as i16)
        } else {
            0
        };
        let integer_min = integer.chars().filter(|&ch| ch == '0').count().max(1) as i16;
        let fraction_min = fraction.chars().filter(|&ch| ch == '0').count() as i16;
        let fraction_max = fraction
            .chars()
            .filter(|&ch| matches!(ch, '0' | '#'))
            .count() as i16;
        let grouping =
            integer.contains(',') || matches!(style, Style::CompactShort | Style::CompactLong);
        Self {
            parse_integer: false,
            grouping,
            decimal_always: digits.ends_with('.'),
            integer_max: if style == Style::Scientific {
                1
            } else {
                2_000_000_000
            },
            integer_min,
            fraction_max,
            fraction_min,
            custom_fraction: false,
            multiplier: if style == Style::Percent || pattern.contains('%') {
                100
            } else if pattern.contains('‰') {
                1000
            } else {
                1
            },
            group_size: if integer.contains(',') { primary } else { 0 },
            second_group_size: secondary,
            custom_group_size: false,
            rounding: 4,
            increment: 0.0,
            width: None,
            pad_position: 0,
            significant: false,
            significant_min: None,
            significant_max: None,
            lenient: false,
        }
    }

    pub fn set_fraction(&mut self, digits: i16) {
        self.fraction_min = digits;
        self.fraction_max = digits;
    }

    fn get(&self, key: i64) -> Option<Value> {
        let number = match key {
            0 => i64::from(self.parse_integer),
            1 => i64::from(self.grouping),
            2 => i64::from(self.decimal_always),
            3 => self.integer_max,
            4 | 5 => i64::from(self.integer_min),
            6 => i64::from(self.fraction_max),
            7 | 8 => i64::from(self.fraction_min),
            9 => self.multiplier,
            10 => i64::from(self.group_size),
            11 => self.rounding,
            12 => return Some(Value::Float(self.increment)),
            13 => i64::from(self.width?),
            14 => self.pad_position,
            15 => i64::from(self.second_group_size),
            16 => i64::from(self.significant),
            17 => i64::from(self.significant_min?),
            18 => i64::from(self.significant_max?),
            19 => i64::from(self.lenient),
            _ => return None,
        };
        Some(Value::Int(number))
    }

    fn set(&mut self, key: i64, value: f64) -> bool {
        let integer = value as i64;
        let digits = integer.clamp(0, 999) as i16;
        match key {
            0 => self.parse_integer = integer != 0,
            1 => self.grouping = integer != 0,
            2 => self.decimal_always = integer != 0,
            3 => self.integer_max = integer.max(0),
            4 | 5 => self.integer_min = digits,
            6 => {
                self.fraction_max = digits;
                self.fraction_min = self.fraction_min.min(digits);
                self.custom_fraction = true;
            }
            7 => {
                self.fraction_min = digits;
                self.fraction_max = self.fraction_max.max(digits);
                self.custom_fraction = true;
            }
            8 => {
                self.set_fraction(digits);
                self.custom_fraction = true;
            }
            9 => self.multiplier = integer,
            10 => {
                self.group_size = digits;
                self.custom_group_size = true;
            }
            11 => self.rounding = integer,
            12 => self.increment = value,
            13 => self.width = Some(digits),
            14 => self.pad_position = integer,
            15 => {
                self.second_group_size = digits;
                self.custom_group_size = true;
            }
            16 => {
                self.significant = integer != 0;
                if self.significant {
                    self.significant_min.get_or_insert(1);
                    self.significant_max.get_or_insert(6);
                }
            }
            17 => {
                self.significant = true;
                self.significant_min = Some(digits.max(1));
            }
            18 => {
                self.significant = true;
                self.significant_max = Some(digits.max(1));
            }
            19 => self.lenient = integer != 0,
            _ => return false,
        }
        true
    }
}

pub fn numeric_pattern(pattern: &str) -> &str {
    let matches_numeric = |ch: char| matches!(ch, '0' | '#' | '.' | ',' | '@');
    let Some(start) = pattern.find(matches_numeric) else {
        return "";
    };
    let rest = &pattern[start..];
    let end = rest
        .find(|ch: char| !matches_numeric(ch))
        .unwrap_or(rest.len());
    &rest[..end]
}

pub fn symbol(state: &Formatter, key: i64) -> Option<String> {
    if let Some(value) = state.symbols.get(&key) {
        return Some(value.clone());
    }
    let data = state.data;
    let text = match key {
        0 => data.symbols[0],
        1 => data.symbols[1],
        2 => ";",
        3 => data.symbols[3],
        4 => return Some(state.ungrouped.format_to_string(&0.into())),
        5 => "#",
        6 => data.symbols[4],
        7 => data.symbols[5],
        8 => return Some(affixes::currency_symbol(state)),
        9 => return Some(state.currency.clone()),
        10 => data.symbols[6],
        11 => data.symbols[7],
        12 => data.symbols[8],
        13 => "*",
        14 => data.symbols[9],
        15 => data.symbols[10],
        16 => "@",
        17 => data.symbols[11],
        _ => return None,
    };
    Some(text.to_owned())
}

fn text_attribute(state: &mut Formatter, key: i64) -> Option<String> {
    if let Some(value) = state.text_attributes.get(&key) {
        return Some(value.clone());
    }
    match key {
        0..=3 => {
            let value = if key < 2 { 1 } else { -1 };
            let output = render::render(state, &value.into(), None)?;
            let (prefix, _, suffix) = affixes::split_number(&output);
            Some(if key % 2 == 0 { prefix } else { suffix }.to_owned())
        }
        4 => Some(" ".to_owned()),
        5 => Some(state.currency.clone()),
        6 => spellout::default_rule(state.style).map(str::to_owned),
        _ => None,
    }
}

pub fn apply(
    context: &mut dyn Context,
    state: &mut Formatter,
    input: &[Value],
    call: Call,
    operation: Operation,
) -> NativeResult {
    let key = args::int(input, 0).unwrap_or(0);
    let result = match operation {
        Operation::GetAttribute => state.attributes.get(key),
        Operation::SetAttribute => state
            .attributes
            .set(key, args::float(input, 1).unwrap_or(0.0))
            .then_some(Value::Bool(true)),
        Operation::GetText => text_attribute(state, key).map(Value::from),
        Operation::SetText => {
            let Ok(value) = std::str::from_utf8(args::bytes(input, 1)) else {
                return call.fail(context, state, 10, "Error converting value to UTF-16");
            };
            if key == 6 && spellout::default_rule(state.style).is_some() {
                if !spellout::accepts_rule(state.style, value) {
                    return call.fail(context, state, 1, "Error setting text attribute");
                }
                if value.is_empty() {
                    state.text_attributes.remove(&key);
                } else {
                    state.text_attributes.insert(key, value.to_owned());
                }
                Some(Value::Bool(true))
            } else if key == 5 {
                state.currency = value.to_ascii_uppercase();
                Some(Value::Bool(true))
            } else if (0..=4).contains(&key) {
                state.text_attributes.insert(key, value.to_owned());
                Some(Value::Bool(true))
            } else {
                None
            }
        }
        Operation::GetSymbol => symbol(state, key).map(Value::from),
        Operation::SetSymbol => {
            let Ok(value) = std::str::from_utf8(args::bytes(input, 1)) else {
                return call.fail(context, state, 10, "Error converting value to UTF-16");
            };
            if (0..=17).contains(&key) {
                state.symbols.insert(key, value.to_owned());
                Some(Value::Bool(true))
            } else {
                None
            }
        }
        Operation::GetPattern => Some(Value::from(state.pattern.clone())),
        Operation::SetPattern => {
            let Ok(pattern) = std::str::from_utf8(args::bytes(input, 0)) else {
                return call.fail(context, state, 10, "Error converting pattern to UTF-16");
            };
            state.attributes = Attributes::new(state.style, pattern);
            state.custom_pattern = true;
            state.pattern = pattern.to_owned();
            Some(Value::Bool(true))
        }
        Operation::Locale if matches!(key, 0 | 1) => Some(Value::from(state.locale_name.clone())),
        Operation::Locale => return call.fail(context, state, 1, "Error getting locale"),
        _ => None,
    };
    match result {
        Some(value) => Ok(value),
        None => {
            let detail = match operation {
                Operation::GetAttribute | Operation::GetText => "Error getting attribute value",
                Operation::SetAttribute | Operation::SetText => "Error setting attribute value",
                Operation::GetSymbol => "Error getting symbol value",
                Operation::SetSymbol => "Error setting symbol value",
                _ => "Unsupported NumberFormatter operation",
            };
            call.fail(context, state, 16, detail)
        }
    }
}
