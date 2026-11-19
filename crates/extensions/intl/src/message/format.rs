//! Format MessageFormat arguments with the intl number, date and plural rules.

use fixed_decimal::{Decimal, FloatPrecision};
use icu_locale::Locale;
use icu_plurals::{PluralCategory, PluralRules};
use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value};
use phera_value::{to_float, to_php_string};

use super::pattern::{Argument, Node};
use crate::{date, number};

pub fn write(
    context: &mut dyn Context,
    locale: &Locale,
    pattern: &[Node],
    values: &PhpArray,
) -> Result<Option<String>, NativeError> {
    let mut output = String::new();
    let mut formatter = Format {
        context,
        locale,
        values,
    };
    Ok(formatter
        .message(pattern, None, &mut output)?
        .then_some(output))
}

struct Format<'a> {
    context: &'a mut dyn Context,
    locale: &'a Locale,
    values: &'a PhpArray,
}

impl Format<'_> {
    fn message(
        &mut self,
        nodes: &[Node],
        pound: Option<&Decimal>,
        output: &mut String,
    ) -> Result<bool, NativeError> {
        for node in nodes {
            match node {
                Node::Text(literal) => output.push_str(literal),
                Node::Pound => {
                    let Some(text) =
                        pound.and_then(|value| number::format_message(self.locale, value, ""))
                    else {
                        return Ok(false);
                    };
                    output.push_str(&text);
                }
                Node::Argument(name, argument) => {
                    let key = name
                        .parse::<i64>()
                        .map_or_else(|_| ArrayKey::str(name.as_str()), ArrayKey::Int);
                    let Some(value) = self.values.get(&key) else {
                        output.push('{');
                        output.push_str(name);
                        output.push('}');
                        continue;
                    };
                    if !self.argument(argument, value, output)? {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    fn argument(
        &mut self,
        kind: &Argument,
        value: &Value,
        output: &mut String,
    ) -> Result<bool, NativeError> {
        let text = match kind {
            Argument::Simple => {
                let text = to_php_string(value, self.context.diagnostics())?;
                std::str::from_utf8(text.as_bytes()).ok().map(str::to_owned)
            }
            Argument::Number(style) => decimal(self.context, value)?
                .and_then(|amount| number::format_message(self.locale, &amount, style)),
            Argument::Date(style) | Argument::Time(style) => {
                let amount = to_float(value, self.context.diagnostics())?;
                date::format_message(
                    self.context,
                    self.locale,
                    amount as i64,
                    style,
                    matches!(kind, Argument::Time(_)),
                )
            }
            Argument::Select(choices) => {
                let text = to_php_string(value, self.context.diagnostics())?;
                return match choice(choices, &String::from_utf8_lossy(text.as_bytes())) {
                    Some(branch) => self.message(branch, None, output),
                    None => Ok(false),
                };
            }
            Argument::Plural {
                choices,
                offset,
                ordinal,
            } => {
                let amount = to_float(value, self.context.diagnostics())?;
                let exact = choices.iter().find(|(key, _)| {
                    key.strip_prefix('=')
                        .and_then(|key| key.parse::<f64>().ok())
                        == Some(amount)
                });
                let Some(adjusted) =
                    Decimal::try_from_f64(amount - offset, FloatPrecision::RoundTrip).ok()
                else {
                    return Ok(false);
                };
                let prefs = self.locale.clone().into();
                let rules = if *ordinal {
                    PluralRules::try_new_ordinal(prefs)
                } else {
                    PluralRules::try_new_cardinal(prefs)
                };
                let category = rules
                    .ok()
                    .map_or(PluralCategory::Other, |rules| rules.category_for(&adjusted));
                let key = match category {
                    PluralCategory::Zero => "zero",
                    PluralCategory::One => "one",
                    PluralCategory::Two => "two",
                    PluralCategory::Few => "few",
                    PluralCategory::Many => "many",
                    PluralCategory::Other => "other",
                };
                let branch = exact
                    .map(|(_, branch)| branch.as_slice())
                    .or_else(|| choice(choices, key));
                return match branch {
                    Some(branch) => self.message(branch, Some(&adjusted), output),
                    None => Ok(false),
                };
            }
        };
        if let Some(text) = text {
            output.push_str(&text);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn decimal(context: &mut dyn Context, value: &Value) -> Result<Option<Decimal>, NativeError> {
    let value = value.deref();
    if let Value::Int(amount) = *value {
        return Ok(Some(amount.into()));
    }
    Ok(Decimal::try_from_f64(
        to_float(&value, context.diagnostics())?,
        FloatPrecision::RoundTrip,
    )
    .ok())
}

fn choice<'a>(choices: &'a [(String, Vec<Node>)], key: &str) -> Option<&'a [Node]> {
    choices
        .iter()
        .find(|(label, _)| label == key)
        .or_else(|| choices.iter().find(|(label, _)| label == "other"))
        .map(|(_, branch)| branch.as_slice())
}
