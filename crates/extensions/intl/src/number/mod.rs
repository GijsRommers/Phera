//! NumberFormatter holds its locale, pattern and attributes per PHP object.
//!
//! ```php
//! $money = new NumberFormatter('nl_BE', NumberFormatter::CURRENCY);
//! $money->formatCurrency(1234.5, 'EUR');
//! ```

mod affixes;
pub(super) use affixes::digit;
mod attributes;
mod constants;
mod parse;
mod registration;
mod render;

use std::collections::HashMap;
use std::rc::Rc;

use icu_decimal::{DecimalFormatter, input::Decimal, options::GroupingStrategy};
use icu_experimental::dimension::currency::formatter::CurrencyFormatter;
use icu_locale::Locale;
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Registry};
use phera_extension_api::{Value, args};

use crate::{error, locale, locale_id, number_data, unsupported::item as unsupported};

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

type Receiver<'a> = Option<&'a Rc<Object>>;
type CurrencyRender = Rc<CurrencyFormatter<DecimalFormatter>>;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Style {
    Pattern,
    Decimal,
    Currency,
    Percent,
    Scientific,
    CurrencyIso,
    CurrencyPlural,
    CurrencyAccounting,
    CurrencyCash,
    CompactShort,
    CompactLong,
}

impl Style {
    fn from_int(style: i64) -> Result<Option<Self>, NativeError> {
        let style = match style {
            0 => Self::Pattern,
            1 => Self::Decimal,
            2 | 16 => Self::Currency,
            3 => Self::Percent,
            4 => Self::Scientific,
            5 => return Err(unsupported("NumberFormatter::SPELLOUT")),
            6 => return Err(unsupported("NumberFormatter::ORDINAL")),
            7 => return Err(unsupported("NumberFormatter::DURATION")),
            9 => return Err(unsupported("NumberFormatter::PATTERN_RULEBASED")),
            10 => Self::CurrencyIso,
            11 => Self::CurrencyPlural,
            12 => Self::CurrencyAccounting,
            13 => Self::CurrencyCash,
            14 => Self::CompactShort,
            15 => Self::CompactLong,
            _ => return Ok(None),
        };
        Ok(Some(style))
    }

    fn is_currency(self) -> bool {
        matches!(
            self,
            Self::Currency
                | Self::CurrencyIso
                | Self::CurrencyPlural
                | Self::CurrencyAccounting
                | Self::CurrencyCash
        )
    }

    fn default_pattern(self, data: &number_data::NumberData) -> &str {
        match self {
            Self::Pattern | Self::CompactShort | Self::CompactLong => "#",
            Self::Decimal => data.decimal,
            Self::Percent => data.percent,
            Self::Scientific => data.scientific,
            Self::Currency | Self::CurrencyCash => data.currency,
            Self::CurrencyAccounting => data.accounting,
            Self::CurrencyIso => data.currency,
            Self::CurrencyPlural => "#,##0.00 ¤¤¤",
        }
    }
}

#[derive(Clone)]
struct Formatter {
    locale: Locale,
    locale_name: String,
    style: Style,
    decimal: DecimalFormatter,
    ungrouped: DecimalFormatter,
    currency: String,
    currency_cache: HashMap<String, CurrencyRender>,
    attributes: attributes::Attributes,
    symbols: HashMap<i64, String>,
    text_attributes: HashMap<i64, String>,
    pattern: String,
    custom_pattern: bool,
    error: (i32, Option<String>),
}

fn init(
    context: &mut dyn Context,
    this: Receiver<'_>,
    arguments: &[Value],
    site: &str,
) -> NativeResult {
    error::clear(context);
    let supplied = args::bytes(arguments, 0);
    if supplied.len() > locale::MAX_LEN {
        return locale::too_long(context, site);
    }
    let locale = locale::effective(context, supplied);
    let locale = locale.split('\0').next().unwrap_or_default();
    let modern = locale_id::cldr(locale);
    let Some(style) = Style::from_int(args::int(arguments, 1).unwrap_or(1))? else {
        return creation_error(context, this, site, 1, "number formatter creation failed");
    };
    if modern.id.language.is_unknown() || !known_language(modern.id.language.as_str()) {
        return Err(NativeError::value_error(format!(
            "{site}(): Argument #1 ($locale) \"{locale}\" is invalid"
        )));
    }
    let Ok(pattern) = std::str::from_utf8(args::bytes(arguments, 2)) else {
        return creation_error(
            context,
            this,
            site,
            10,
            "error converting pattern to UTF-16",
        );
    };
    let Some(state) = make_formatter(modern, resolve_locale(locale), style, pattern) else {
        return creation_error(context, this, site, 2, "number formatter creation failed");
    };
    if let Some(object) = this {
        let mut payload = object.native_payload_mut().map_err(|_| borrowed())?;
        if payload.is_some() {
            return Err(NativeError::error(
                "NumberFormatter object is already constructed",
            ));
        }
        *payload = Some(Box::new(state));
        Ok(Value::Null)
    } else {
        let value = context.create_object("NumberFormatter", PhpArray::new())?;
        if let Value::Object(object) = &value {
            *object.native_payload_mut().map_err(|_| borrowed())? = Some(Box::new(state));
        }
        Ok(value)
    }
}

fn make_formatter(
    modern: Locale,
    locale_name: String,
    style: Style,
    pattern: &str,
) -> Option<Formatter> {
    let code = number_data::currency(&modern);
    let data = number_data::for_locale(&modern);
    let decimal =
        DecimalFormatter::try_new(modern.clone().into(), GroupingStrategy::Auto.into()).ok()?;
    let ungrouped =
        DecimalFormatter::try_new(modern.clone().into(), GroupingStrategy::Never.into()).ok()?;
    let custom_pattern = !pattern.is_empty() && style == Style::Pattern;
    let base = if custom_pattern {
        pattern.to_owned()
    } else if style == Style::CurrencyIso {
        data.currency.replace('¤', "¤¤")
    } else {
        style.default_pattern(data).to_owned()
    };
    let mut state = Formatter {
        locale: modern,
        locale_name,
        style,
        decimal,
        ungrouped,
        currency: code,
        currency_cache: HashMap::new(),
        attributes: attributes::Attributes::new(style, &base),
        symbols: HashMap::new(),
        text_attributes: HashMap::new(),
        pattern: base,
        custom_pattern,
        error: (0, None),
    };
    if style.is_currency() {
        let default_currency = state.currency.clone();
        if let Some(text) =
            affixes::currency_value(&mut state, &Decimal::from(1), &default_currency)
        {
            let (_, number, _) = affixes::split_number(&text);
            if let Some((_, fraction)) = number.split_once(data.symbols[6]) {
                let digits = fraction
                    .chars()
                    .filter(|scalar| affixes::digit(*scalar).is_some())
                    .count();
                state.attributes.set_fraction(digits as i16);
            } else {
                state.attributes.set_fraction(0);
            }
        }
    }
    Some(state)
}

pub fn format_message(locale: &Locale, amount: &Decimal, style: &str) -> Option<String> {
    let (style, pattern) = match style {
        "" => (Style::Decimal, ""),
        "integer" => (Style::Pattern, "#,##0"),
        "currency" => (Style::Currency, ""),
        "percent" => (Style::Percent, ""),
        pattern => (Style::Pattern, pattern),
    };
    let mut formatter = make_formatter(locale.clone(), locale.id.to_string(), style, pattern)?;
    render::render(&mut formatter, amount, None)
}

pub(crate) fn known_language(language: &str) -> bool {
    include_str!("../../data/cldr-locales.txt")
        .lines()
        .any(|candidate| {
            candidate == language
                || candidate
                    .split_once('_')
                    .is_some_and(|(base, _)| base == language)
        })
}

pub(crate) fn resolve_locale(locale: &str) -> String {
    let id = locale_id::canonical(locale);
    let base = id.split('@').next().unwrap_or_default();
    let mut candidate = base;
    loop {
        if crate::locale_matching::is_available(candidate) {
            return candidate.to_owned();
        }
        let Some((parent, _)) = candidate.rsplit_once('_') else {
            return "root".to_owned();
        };
        candidate = parent;
    }
}

fn creation_error(
    context: &mut dyn Context,
    this: Receiver<'_>,
    site: &str,
    code: i32,
    message: &str,
) -> NativeResult {
    let result = error::fail(context, site, code, message, Value::Null);
    if this.is_some() {
        Err(NativeError::throw(
            "IntlException",
            format!("{site}(): {message}"),
        ))
    } else {
        result
    }
}

fn borrowed() -> NativeError {
    NativeError::error("NumberFormatter is already being modified")
}

fn dispatch(
    context: &mut dyn Context,
    this: Receiver<'_>,
    input: &[Value],
    site: &'static str,
    operation: Operation,
) -> NativeResult {
    let (receiver, arguments) = match this {
        Some(object) => (Some(object), input),
        None => (args::object(input, 0), input.get(1..).unwrap_or_default()),
    };
    let Some(receiver) = receiver else {
        return Ok(Value::Bool(false));
    };
    let mut payload = receiver.native_payload_mut().map_err(|_| borrowed())?;
    let Some(state) = payload
        .as_mut()
        .and_then(|data| data.downcast_mut::<Formatter>())
    else {
        return invalid_receiver(context, operation, site);
    };
    match operation {
        Operation::ErrorCode => return Ok(Value::Int(i64::from(state.error.0))),
        Operation::ErrorMessage => {
            return Ok(Value::from(match &state.error.1 {
                Some(detail) => format!("{detail}: {}", error::name(state.error.0)),
                None => error::name(state.error.0).to_owned(),
            }));
        }
        _ => {
            error::clear(context);
            state.error = (0, None);
        }
    }
    let action = Call {
        site,
        argument_shift: usize::from(this.is_none()),
    };
    match operation {
        Operation::Format => render::format(context, state, arguments, action),
        Operation::FormatCurrency => render::format_currency(context, state, arguments, action),
        Operation::Parse => parse::number(context, state, arguments, action),
        Operation::ParseCurrency => parse::currency(context, state, arguments, action),
        _ => attributes::apply(context, state, arguments, action, operation),
    }
}

fn invalid_receiver(context: &mut dyn Context, operation: Operation, site: &str) -> NativeResult {
    match operation {
        Operation::ErrorCode => Ok(Value::Int(0)),
        Operation::ErrorMessage => Ok(Value::from("U_ZERO_ERROR")),
        _ => error::fail(
            context,
            site,
            1,
            "Found unconstructed NumberFormatter",
            Value::Bool(false),
        ),
    }
}

#[derive(Clone, Copy)]
enum Operation {
    Format,
    FormatCurrency,
    Parse,
    ParseCurrency,
    GetAttribute,
    SetAttribute,
    GetText,
    SetText,
    GetSymbol,
    SetSymbol,
    GetPattern,
    SetPattern,
    Locale,
    ErrorCode,
    ErrorMessage,
}

#[derive(Clone, Copy)]
struct Call {
    site: &'static str,
    argument_shift: usize,
}

impl Call {
    fn argument_error(self, argument: usize, name: &str, detail: &str) -> NativeError {
        NativeError::value_error(format!(
            "{}(): Argument #{} (${name}) {detail}",
            self.site,
            argument + self.argument_shift,
        ))
    }

    fn fail(
        self,
        context: &mut dyn Context,
        state: &mut Formatter,
        code: i32,
        detail: &str,
    ) -> NativeResult {
        state.error = (code, Some(format!("{}(): {detail}", self.site)));
        error::fail(context, self.site, code, detail, Value::Bool(false))
    }
}
