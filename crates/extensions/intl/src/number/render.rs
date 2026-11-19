//! Numeric formatting combines fixed-decimal arithmetic and ICU4X locale data.
//!
//! ```php
//! $percent = new NumberFormatter('fr_FR', NumberFormatter::PERCENT);
//! $percent->format(0.25); // "25 %"
//! ```

use fixed_decimal::{
    Decimal, FloatPrecision, RoundingIncrement, Sign, SignedRoundingMode, UnsignedRoundingMode,
};
use icu_decimal::CompactDecimalFormatter;
use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use super::affixes::{affixes, currency_value, digit, pattern_affixes, split_number};
use super::{Call, Formatter, Style, attributes};
use crate::number_data;

pub fn format(
    context: &mut dyn Context,
    state: &mut Formatter,
    arguments: &[Value],
    call: Call,
) -> NativeResult {
    let format_type = args::int(arguments, 1).unwrap_or(0);
    let original = args::value(arguments, 0);
    let numeric = match format_type {
        0 => original.clone(),
        1 => Value::Int(number_as_int(original) as i32 as i64),
        2 => Value::Int(number_as_int(original)),
        3 => Value::Float(args::float(arguments, 0).unwrap_or(0.0)),
        _ => return Err(type_error(call, format_type, false)),
    };
    result(context, state, numeric, None, call)
}

pub fn format_currency(
    context: &mut dyn Context,
    state: &mut Formatter,
    arguments: &[Value],
    call: Call,
) -> NativeResult {
    let Ok(currency) = std::str::from_utf8(args::bytes(arguments, 1)) else {
        return call.fail(context, state, 10, "Currency conversion to UTF-16 failed");
    };
    let amount = Value::Float(args::float(arguments, 0).unwrap_or(0.0));
    result(context, state, amount, Some(currency), call)
}

pub fn type_error(call: Call, kind: i64, parse: bool) -> NativeError {
    let action = match (parse, call.argument_shift) {
        (true, 0) => "parseCurrency() method",
        (false, 0) => "formatCurrency() method",
        (true, _) => "numfmt_parse_currency() function",
        (false, _) => "numfmt_format_currency() function",
    };
    if kind == 4 {
        let class = if call.argument_shift == 0 {
            "NumberFormatter::"
        } else {
            ""
        };
        let detail = format!(
            "cannot be NumberFormatter::TYPE_CURRENCY constant, use {class}{action} instead"
        );
        call.argument_error(2, "type", &detail)
    } else {
        call.argument_error(2, "type", "must be a NumberFormatter::TYPE_* constant")
    }
}

fn number_as_int(number: &Value) -> i64 {
    match number {
        Value::Int(integer) => *integer,
        Value::Float(float) => *float as i64,
        _ => 0,
    }
}

fn result(
    context: &mut dyn Context,
    state: &mut Formatter,
    number: Value,
    currency: Option<&str>,
    call: Call,
) -> NativeResult {
    let value = match number {
        Value::Int(integer) => Some(Decimal::from(integer)),
        Value::Float(number) if number.is_finite() => {
            Decimal::try_from_f64(number, FloatPrecision::RoundTrip).ok()
        }
        Value::Float(number) => return Ok(Value::from(nonfinite(state, number))),
        _ => None,
    };
    if let Some(text) = value.and_then(|value| render(state, &value, currency)) {
        Ok(Value::from(text))
    } else {
        call.fail(context, state, 1, "Number formatting failed")
    }
}

fn nonfinite(state: &Formatter, value: f64) -> String {
    let symbol =
        attributes::symbol(state, if value.is_nan() { 15 } else { 14 }).unwrap_or_default();
    if value.is_sign_negative() && !value.is_nan() {
        attributes::symbol(state, 6).unwrap_or_default() + &symbol
    } else {
        symbol
    }
}

pub fn render(state: &mut Formatter, original: &Decimal, currency: Option<&str>) -> Option<String> {
    let normalized_code;
    let currency = if let Some(code) = currency {
        normalized_code = code.to_ascii_uppercase();
        normalized_code.as_str()
    } else {
        &state.currency.clone()
    };
    let mut value = original.clone();
    match state.attributes.multiplier {
        1 => {}
        100 => value.multiply_pow10(2),
        1000 => value.multiply_pow10(3),
        multiplier => {
            let float = original.to_string().parse::<f64>().ok()? * multiplier as f64;
            value = Decimal::try_from_f64(float, FloatPrecision::RoundTrip).ok()?;
        }
    }
    if state.style.is_currency() && !state.custom_pattern {
        let mut currency_number = value.clone();
        let (digits, cash_rounding) =
            number_data::fraction(currency, state.style == Style::CurrencyCash);
        let rounding = rounding_mode(state.attributes.rounding);
        let increment = match cash_rounding {
            2 => RoundingIncrement::MultiplesOf2,
            5 => RoundingIncrement::MultiplesOf5,
            25 => RoundingIncrement::MultiplesOf25,
            _ => RoundingIncrement::MultiplesOf1,
        };
        currency_number.round_with_mode_and_increment(-digits, rounding, increment);
        let output = currency_value(state, &currency_number, currency)?;
        let data = number_data::for_locale(&state.locale);
        let needs_override = state.attributes.custom_fraction
            || !state.attributes.grouping
            || state.attributes.custom_group_size
            || !state.symbols.is_empty()
            || !state.text_attributes.is_empty();
        let (prefix, number, suffix) = split_number(&output);
        if !needs_override {
            if state.attributes.grouping
                && data.minimum_grouping > 1
                && !data.symbols[11].is_empty()
                && !number.contains(data.symbols[11])
            {
                let grouped = regroup(number, data.symbols[6], data.symbols[11], state);
                return Some(format!("{prefix}{grouped}{suffix}"));
            }
            return Some(output);
        }
        if state.attributes.custom_fraction {
            round(state, &mut value);
        } else {
            value = currency_number;
        }
        let magnitude = Decimal::new(Sign::None, value.absolute);
        let formatted = decimal(state, &magnitude);
        return Some(affixes(state, original, prefix, &formatted, suffix));
    }
    if matches!(state.style, Style::CompactShort | Style::CompactLong) && !state.custom_pattern {
        let preferences = state.locale.clone().into();
        let formatter = if state.style == Style::CompactShort {
            CompactDecimalFormatter::try_new_short(preferences, Default::default())
        } else {
            CompactDecimalFormatter::try_new_long(preferences, Default::default())
        }
        .ok()?;
        return Some(formatter.format(&value).to_string());
    }
    let scientific = state.style == Style::Scientific || state.pattern.contains("E0");
    let mut exponent = 0;
    if scientific && !value.absolute.is_zero() {
        exponent = value.absolute.nonzero_magnitude_start();
        value.multiply_pow10(-exponent);
        value.absolute.trim_start();
    }
    if state.custom_pattern
        || state.attributes.custom_fraction
        || (!scientific && state.style != Style::Pattern)
    {
        round(state, &mut value);
    }

    let negative = value.sign == Sign::Negative;
    value.sign = Sign::None;
    let mut number = decimal(state, &value);
    if scientific {
        let symbol = attributes::symbol(state, 11).unwrap_or_else(|| "E".to_owned());
        number.push_str(&symbol);
        number.push_str(&state.ungrouped.format_to_string(&Decimal::from(exponent)));
    }
    let (prefix, suffix) = pattern_affixes(state, currency, negative);
    Some(affixes(state, original, &prefix, &number, &suffix))
}

fn round(state: &Formatter, value: &mut Decimal) {
    let settings = &state.attributes;
    let upper = value.absolute.nonzero_magnitude_start();
    let (lower, padding) = if settings.significant {
        (
            upper.saturating_sub(settings.significant_max.unwrap_or(6) - 1),
            upper.saturating_sub(settings.significant_min.unwrap_or(1) - 1),
        )
    } else {
        (-settings.fraction_max, -settings.fraction_min)
    };
    let mode = rounding_mode(settings.rounding);
    value.round_with_mode(lower, mode);
    value.absolute.trim_end();
    value.absolute.pad_end(padding.min(0));
    value.absolute.pad_start(settings.integer_min);
}

fn rounding_mode(settings: i64) -> SignedRoundingMode {
    match settings {
        0 => SignedRoundingMode::Ceil,
        1 => SignedRoundingMode::Floor,
        2 => SignedRoundingMode::Unsigned(UnsignedRoundingMode::Trunc),
        3 => SignedRoundingMode::Unsigned(UnsignedRoundingMode::Expand),
        5 => SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfTrunc),
        6 => SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfExpand),
        _ => SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfEven),
    }
}

fn decimal(state: &Formatter, value: &Decimal) -> String {
    let settings = &state.attributes;
    let data = number_data::for_locale(&state.locale);
    let manual_grouping = settings.custom_group_size || data.minimum_grouping > 1;
    let ungrouped = !settings.grouping || manual_grouping;
    let formatter = if ungrouped {
        &state.ungrouped
    } else {
        &state.decimal
    };
    let mut rendered = formatter.format_to_string(value);
    if manual_grouping && settings.grouping && settings.group_size > 0 {
        rendered = regroup(&rendered, data.symbols[0], data.symbols[1], state);
    }
    if settings.decimal_always && !rendered.contains(data.symbols[0]) {
        rendered.push_str(data.symbols[0]);
    }
    for (key, base) in [(0, data.symbols[0]), (1, data.symbols[1]), (4, "")] {
        if let Some(replacement) = state.symbols.get(&key) {
            if key == 4 {
                if let Some(zero) = replacement.chars().next() {
                    rendered = rendered
                        .chars()
                        .map(|ch| {
                            digit(ch)
                                .and_then(|value| char::from_u32(u32::from(zero) + value))
                                .unwrap_or(ch)
                        })
                        .collect();
                }
            } else if !base.is_empty() {
                rendered = rendered.replace(base, replacement);
            }
        }
    }
    rendered
}

fn regroup(text: &str, decimal: &str, separator: &str, state: &Formatter) -> String {
    let (integer, fraction) = text.split_once(decimal).unwrap_or((text, ""));
    let total = integer.chars().count();
    let primary = state.attributes.group_size as usize;
    let secondary = if state.attributes.second_group_size > 0 {
        state.attributes.second_group_size as usize
    } else {
        primary
    };
    if primary == 0 || total <= primary {
        return text.to_owned();
    }
    let mut number = String::new();
    for (index, digit) in integer.chars().enumerate() {
        let remaining = total - index;
        if index > 0 && remaining >= primary && (remaining - primary).is_multiple_of(secondary) {
            number.push_str(separator);
        }
        number.push(digit);
    }
    number.push_str(
        &text[integer.len()..integer.len() + usize::from(!fraction.is_empty()) * decimal.len()],
    );
    number.push_str(fraction);
    number
}
