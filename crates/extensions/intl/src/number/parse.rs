//! Parsing reads locale digits and separators and returns a UTF-8 byte offset.
//!
//! ```php
//! $fmt = new NumberFormatter('nl_BE', NumberFormatter::DECIMAL);
//! $offset = 0;
//! $fmt->parse('12,50 EUR', NumberFormatter::TYPE_DOUBLE, $offset); // 12.5, 5
//! ```

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use super::{Call, Formatter, affixes, attributes, render};

pub fn number(
    context: &mut dyn Context,
    state: &mut Formatter,
    arguments: &[Value],
    call: Call,
) -> NativeResult {
    let format_type = args::int(arguments, 1).unwrap_or(3);
    let Ok(text) = std::str::from_utf8(args::bytes(arguments, 0)) else {
        return call.fail(context, state, 10, "String conversion to UTF-16 failed");
    };
    let start = position(arguments, 2);
    let in_bounds = (0..=text.len() as i64).contains(&start);
    if in_bounds && !text.is_char_boundary(start as usize) {
        return call.fail(context, state, 10, "Invalid UTF-8 offset");
    }
    if !(1..=3).contains(&format_type) {
        return Err(render::type_error(call, format_type, true));
    }
    if !in_bounds {
        return Ok(if format_type == 3 {
            Value::Float(0.0)
        } else {
            Value::Int(0)
        });
    }
    let start = start as usize;
    let remainder = &text[start..];
    let Some((parsed, length)) = scan(state, remainder) else {
        assign(arguments, 2, Value::Int(start as i64))?;
        return call.fail(context, state, 9, "Number parsing failed");
    };
    assign(arguments, 2, Value::Int((start + length) as i64))?;
    let result = match format_type {
        1 => parsed
            .parse::<i32>()
            .map(|value| i64::from(value).into())
            .ok()
            .or_else(|| integral_float(&parsed, i32::MIN.into(), i32::MAX.into())),
        2 => parsed
            .parse::<i64>()
            .map(Value::Int)
            .ok()
            .or_else(|| integral_float(&parsed, i64::MIN as f64, i64::MAX as f64 - 1024.0)),
        _ => parsed.parse::<f64>().ok().map(Value::Float),
    };
    match result {
        Some(value) => Ok(value),
        None => call.fail(context, state, 3, "Number parsing failed"),
    }
}

fn integral_float(parsed: &str, min: f64, max: f64) -> Option<Value> {
    let number = parsed.parse::<f64>().ok()?;
    (min..=max)
        .contains(&number)
        .then_some(Value::Int(number.trunc() as i64))
}

pub fn currency(
    context: &mut dyn Context,
    state: &mut Formatter,
    arguments: &[Value],
    call: Call,
) -> NativeResult {
    let Ok(text) = std::str::from_utf8(args::bytes(arguments, 0)) else {
        return call.fail(context, state, 10, "String conversion to UTF-16 failed");
    };
    let start = position(arguments, 2);
    if !(0..=text.len() as i64).contains(&start) {
        return Ok(Value::Bool(false));
    }
    let start = start as usize;
    let Some(remainder) = text.get(start..) else {
        return call.fail(context, state, 10, "Invalid UTF-8 offset");
    };
    let Some((code, parsed, length)) = scan_currency(state, remainder) else {
        assign(arguments, 2, Value::Int(start as i64))?;
        return call.fail(context, state, 9, "Number parsing failed");
    };
    assign(arguments, 2, Value::Int((start + length) as i64))?;
    assign(arguments, 1, Value::from(code))?;
    Ok(Value::Float(parsed))
}

fn scan_currency(state: &Formatter, text: &str) -> Option<(String, f64, usize)> {
    let symbol = affixes::currency_symbol(state);
    let currencies = [
        (symbol.as_str(), state.currency.as_str()),
        ("€", "EUR"),
        ("£", "GBP"),
        ("$", "USD"),
        ("¥", "JPY"),
        ("￥", "JPY"),
        ("₹", "INR"),
        ("₽", "RUB"),
    ];
    let trim = |scalar: char| scalar.is_whitespace() || matches!(scalar, '\u{200f}' | '\u{200e}');
    let mut input = text.trim_start_matches(trim);
    let mut sign = 1.0;
    if let Some(rest) = input.strip_prefix('-') {
        input = rest;
        sign = -1.0;
    }
    if let Some((code, tail)) = currency_token(input, &currencies) {
        let tail = tail.trim_start_matches(trim);
        let (parsed, length) = scan(state, tail)?;
        return Some((
            code,
            parsed.parse::<f64>().ok()? * sign,
            text.len() - tail.len() + length,
        ));
    }
    let (parsed, length) = scan(state, input)?;
    let rest = input.get(length..)?.trim_start_matches(trim);
    let (code, tail) = currency_token(rest, &currencies)?;
    Some((
        code,
        parsed.parse::<f64>().ok()? * sign,
        text.len() - tail.len(),
    ))
}

fn currency_token<'a>(text: &'a str, known: &[(&str, &str)]) -> Option<(String, &'a str)> {
    for &(symbol, code) in known {
        if !symbol.is_empty()
            && let Some(rest) = text.strip_prefix(symbol)
        {
            return Some((code.to_owned(), rest));
        }
    }
    let code = text.get(..3)?;
    if code.bytes().all(|byte| byte.is_ascii_uppercase()) {
        Some((code.to_owned(), &text[3..]))
    } else {
        None
    }
}

fn scan(state: &Formatter, text: &str) -> Option<(String, usize)> {
    let decimal = attributes::symbol(state, if state.style.is_currency() { 10 } else { 0 })?;
    let group = attributes::symbol(state, if state.style.is_currency() { 17 } else { 1 })?;
    let minus = attributes::symbol(state, 6)?;
    let plus = attributes::symbol(state, 7)?;
    let mut input = text;
    let mut number = String::new();
    if let Some(tail) = input
        .strip_prefix(&minus)
        .or_else(|| input.strip_prefix('-'))
    {
        number.push('-');
        input = tail;
    } else if let Some(tail) = input.strip_prefix(&plus) {
        input = tail;
    }
    let mut has_digit = false;
    let mut has_decimal = false;
    while let Some(scalar) = input.chars().next() {
        if let Some(digit) = affixes::digit(scalar).and_then(|value| char::from_digit(value, 10)) {
            number.push(digit);
            input = &input[scalar.len_utf8()..];
            has_digit = true;
        } else if !has_decimal && !decimal.is_empty() && input.starts_with(&decimal) {
            if state.attributes.parse_integer {
                break;
            }
            number.push('.');
            has_decimal = true;
            input = &input[decimal.len()..];
        } else if !has_decimal
            && has_digit
            && !group.is_empty()
            && state.attributes.grouping
            && let Some(rest) = input.strip_prefix(&group)
            && rest.chars().next().and_then(affixes::digit).is_some()
        {
            input = rest;
        } else {
            break;
        }
    }
    if !has_digit {
        return None;
    }
    if let Some(rest) = input.strip_prefix(['e', 'E']) {
        let digits = rest.trim_start_matches(['+', '-']);
        let count = digits
            .bytes()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if count > 0 {
            let length = rest.len() - digits.len() + count;
            number.push('e');
            number.push_str(&rest[..length]);
            input = &rest[length..];
        }
    }
    if state.attributes.multiplier != 1 {
        let value = number.parse::<f64>().ok()? / state.attributes.multiplier as f64;
        number = value.to_string();
    }
    if state.style == super::Style::Percent {
        let percent = attributes::symbol(state, 3)?;
        let tail = input.trim_start();
        if let Some(rest) = tail.strip_prefix(&percent) {
            input = rest;
        } else if !state.attributes.lenient {
            return None;
        }
    }
    Some((number, text.len() - input.len()))
}

fn position(arguments: &[Value], index: usize) -> i64 {
    args::reference(arguments, index)
        .and_then(|value| {
            let value = value.try_borrow().ok()?;
            let offset = match &*value {
                Value::Int(value) => *value,
                Value::Float(value) => *value as i64,
                Value::String(value) => std::str::from_utf8(value.as_bytes())
                    .ok()
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or(0),
                _ => 0,
            };
            Some(i64::from(offset as i32))
        })
        .unwrap_or(0)
}

fn assign(arguments: &[Value], index: usize, value: Value) -> Result<(), NativeError> {
    if let Some(reference) = args::reference(arguments, index) {
        *reference.try_borrow_mut().map_err(|_| {
            NativeError::error("NumberFormatter argument is already being modified")
        })? = value;
    }
    Ok(())
}
