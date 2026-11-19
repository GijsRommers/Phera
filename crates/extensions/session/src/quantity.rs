//! `ini_set('session.gc_maxlifetime', '2k')` sets the lifetime to 2048 seconds.

use std::fmt::Write;

pub fn parse(input: &[u8]) -> (i64, Option<String>) {
    let start = input.trim_ascii_start();
    let text = start.trim_ascii_end();
    if text.is_empty() {
        return (0, None);
    }
    let leading = input.len() - start.len();
    let sign = usize::from(matches!(text.first(), Some(b'+' | b'-')));
    let negative = text.first() == Some(&b'-');
    let mut digits = &text[sign..];
    if !digits.first().is_some_and(u8::is_ascii_digit) {
        return invalid(input, "no valid leading digits");
    }
    let mut base = 10;
    let mut prefix = 0;
    if let [b'0', byte, ..] = digits {
        base = match byte {
            b'x' | b'X' => 16,
            b'b' | b'B' => 2,
            b'o' | b'O' => 8,
            b'0'..=b'9' => 8,
            b'g' | b'G' | b'm' | b'M' | b'k' | b'K' => 10,
            byte => {
                return (
                    0,
                    Some(format!(
                        "Invalid prefix \"0{}\", interpreting as \"0\" for backwards compatibility",
                        escaped(&[*byte])
                    )),
                );
            }
        };
        if matches!(byte, b'x' | b'X' | b'b' | b'B' | b'o' | b'O') {
            prefix = 2;
            digits = &digits[prefix..];
            if digits.is_empty()
                || digits
                    .first()
                    .is_some_and(|byte| byte.is_ascii_whitespace() || matches!(byte, b'+' | b'-'))
            {
                return invalid(input, "no digits after base prefix");
            }
        }
    }
    let mut unsigned = 0u64;
    let mut count = 0;
    let mut overflow = false;
    for digit in digits
        .iter()
        .map_while(|byte| char::from(*byte).to_digit(base))
    {
        count += 1;
        match unsigned
            .checked_mul(u64::from(base))
            .and_then(|value| value.checked_add(u64::from(digit)))
        {
            Some(value) => unsigned = value,
            None => {
                unsigned = u64::MAX;
                overflow = true;
            }
        }
    }
    if count == 0 {
        return invalid(input, "no valid leading digits");
    }
    let mut number = unsigned as i64;
    if negative && unsigned == 1 << 63 {
        number = i64::MIN;
    } else if number < 0 {
        overflow = true;
    } else if negative {
        number = -number;
    }
    let remaining = &digits[count..];
    let suffix = remaining.trim_ascii_start();
    if let Some(&last) = suffix.last() {
        let split = leading + sign + prefix + count + (remaining.len() - suffix.len());
        let factor = match last {
            b'g' | b'G' => 1 << 30,
            b'm' | b'M' => 1 << 20,
            b'k' | b'K' => 1 << 10,
            _ => {
                return (
                    number,
                    Some(format!(
                        "Invalid quantity \"{}\": unknown multiplier \"{}\", \
                         interpreting as \"{}\" \
                 for backwards compatibility",
                        escaped(input),
                        escaped(&[last]),
                        escaped(&input[..split])
                    )),
                );
            }
        };
        let (scaled, multiplied_overflow) = number.overflowing_mul(factor);
        number = scaled;
        overflow |= multiplied_overflow;
        if suffix.len() > 1 {
            return (
                number,
                Some(format!(
                    "Invalid quantity \"{}\", interpreting as \"{}{}\" for backwards compatibility",
                    escaped(input),
                    escaped(&input[..split]),
                    escaped(&[last])
                )),
            );
        }
    }
    let warning = overflow.then(|| {
        format!(
            "Invalid quantity \"{}\": value is out of range, using overflow result \
         for backwards compatibility",
            escaped(input)
        )
    });
    (number, warning)
}

fn invalid(input: &[u8], reason: &str) -> (i64, Option<String>) {
    (
        0,
        Some(format!(
            "Invalid quantity \"{}\": {reason}, interpreting as \"0\" for backwards compatibility",
            escaped(input)
        )),
    )
}

fn escaped(bytes: &[u8]) -> String {
    let mut text = String::new();
    for byte in bytes {
        match byte {
            b'\n' => text.push_str("\\n"),
            b'\r' => text.push_str("\\r"),
            b'\t' => text.push_str("\\t"),
            b'\\' => text.push_str("\\\\"),
            32..=126 => text.push(char::from(*byte)),
            _ => {
                let _ = write!(text, "\\x{byte:02x}");
            }
        }
    }
    text
}
