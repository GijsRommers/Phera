//! Recover indexed values from a MessageFormat message with literal delimiters.

use icu_locale::Locale;
use phera_extension_api::{ArrayKey, Context, PhpArray, Value};

use super::pattern::{Argument, Node};

pub fn read(
    context: &mut dyn Context,
    locale: &Locale,
    pattern: &[Node],
    input: &[u8],
) -> Result<PhpArray, i32> {
    let mut input = std::str::from_utf8(input).map_err(|_| 10)?;
    let mut values = PhpArray::new();
    for (index, node) in pattern.iter().enumerate() {
        let (key, kind) = match node {
            Node::Text(literal) => {
                input = input.strip_prefix(literal).ok_or(6)?;
                continue;
            }
            Node::Argument(key, kind) => (key.parse::<i64>().map_err(|_| 65804)?, kind),
            Node::Pound => return Err(6),
        };
        let (value, rest) = match pattern.get(index + 1) {
            Some(Node::Text(delimiter)) => {
                let end = input.find(delimiter).ok_or(6)?;
                input.split_at(end)
            }
            Some(_) | None => (input, ""),
        };
        input = rest;
        let value = match kind {
            Argument::Simple => Value::from(value),
            Argument::Number(style) => {
                crate::number::parse_message(locale, value, style).ok_or(6)?
            }
            Argument::Date(style) | Argument::Time(style) => Value::Float(
                crate::date::parse_message(
                    context,
                    locale,
                    value,
                    style,
                    matches!(kind, Argument::Time(_)),
                )
                .ok_or(6)?,
            ),
            _ => return Err(6),
        };
        values.insert(ArrayKey::Int(key), value);
    }
    Ok(values)
}
