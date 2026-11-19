//! `setlocale(LC_NUMERIC, 0)` reads the request's current numeric locale.
//!
//! `setlocale(LC_ALL, 'nl_BE.UTF-8')` returns that name and stores it for every
//! category. Acceptance uses locale name syntax so it is independent of the
//! installed libc locales. The only effects today are on subsequent locale
//! queries and CSV character scanning under UTF-8 LC_CTYPE. `localeconv()`
//! and `strcoll()` implement the C/POSIX conventions.
//! Localized collation, decimal and monetary symbols, and time or message
//! formats would need locale data such as ICU's. PHP float-to-string conversion
//! is locale independent, including after setting LC_NUMERIC.

use phera_extension_api::value::PhpString;
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

mod state;

pub(crate) use state::ctype_name;
use state::{ALL_ID, Category, c_string, select};

pub(crate) fn register(registry: &mut Registry) {
    registry.constant("LC_ALL", ALL_ID);
    for category in Category::ALL {
        registry.constant(category.name(), category.id());
    }
    registry.function(FunctionEntry::asynchronous(
        "setlocale",
        const {
            &[
                Param::new("category", ParamType::Int),
                Param::new("locales", ParamType::ArrayOrString).nullable(),
                Param::new("rest", ParamType::ArrayOrString)
                    .nullable()
                    .variadic(),
            ]
        },
        setlocale,
    ));
    registry.function(FunctionEntry::sync("localeconv", &[], localeconv));
    registry.function(FunctionEntry::sync(
        "strcoll",
        const {
            &[
                Param::new("string1", ParamType::String),
                Param::new("string2", ParamType::String),
            ]
        },
        strcoll,
    ));
    registry.describe_function("setlocale", "string|false", None);
    registry.describe_function("localeconv", "array", None);
    registry.describe_function("strcoll", "int", None);
}

fn setlocale(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let id = args::int(&arguments, 0).unwrap_or_default();
        let category = Category::ALL
            .into_iter()
            .find(|category| category.id() == id);
        for candidate in arguments.iter().skip(1) {
            if let Value::Array(candidates) = candidate {
                for value in candidates.values() {
                    let name = context.stringify(value).await?;
                    if let Some(selected) = select(context, id, category, &name) {
                        return Ok(Value::String(selected));
                    }
                }
            } else {
                let name = context.stringify(candidate).await?;
                if let Some(selected) = select(context, id, category, &name) {
                    return Ok(Value::String(selected));
                }
            }
        }
        Ok(Value::Bool(false))
    })
}

fn localeconv(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut data = PhpArray::new();
    data.insert(ArrayKey::str("decimal_point"), Value::from("."));
    for key in [
        "thousands_sep",
        "int_curr_symbol",
        "currency_symbol",
        "mon_decimal_point",
        "mon_thousands_sep",
        "positive_sign",
        "negative_sign",
    ] {
        data.insert(ArrayKey::str(key), Value::from(""));
    }
    for key in [
        "int_frac_digits",
        "frac_digits",
        "p_cs_precedes",
        "p_sep_by_space",
        "n_cs_precedes",
        "n_sep_by_space",
        "p_sign_posn",
        "n_sign_posn",
    ] {
        data.insert(
            ArrayKey::str(key),
            Value::Int(i64::from(std::ffi::c_char::MAX)),
        );
    }
    for key in ["grouping", "mon_grouping"] {
        data.insert(ArrayKey::str(key), Value::from(PhpArray::new()));
    }
    Ok(Value::from(data))
}

fn strcoll(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = |index| args::string(arguments, index).map_or(&[][..], PhpString::as_bytes);
    let compare = |left, right| i64::from(left) - i64::from(right);
    let left = c_string(string(0));
    let right = c_string(string(1));
    let mismatch = left.iter().zip(right).find(|(left, right)| left != right);
    let difference = mismatch.map_or_else(
        || {
            compare(
                left.get(right.len()).copied().unwrap_or_default(),
                right.get(left.len()).copied().unwrap_or_default(),
            )
        },
        |(&left, &right)| compare(left, right),
    );
    Ok(Value::Int(difference))
}
