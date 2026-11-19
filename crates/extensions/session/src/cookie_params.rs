//! `session_set_cookie_params(['path' => '/app', 'httponly' => true])` configures the next cookie.

use phera_extension_api::value::to_bool;
use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, PhpString, Value, args,
};

use crate::{id::c_string, settings, state::can_change};

const PARAMETERS: &[Param] = &[
    Param::new("lifetime_or_options", ParamType::ArrayOrInt),
    Param::new("path", ParamType::String)
        .nullable()
        .with_default(ConstantValue::Null),
    Param::new("domain", ParamType::String)
        .nullable()
        .with_default(ConstantValue::Null),
    Param::new("secure", ParamType::Bool)
        .nullable()
        .with_default(ConstantValue::Null),
    Param::new("httponly", ParamType::Bool)
        .nullable()
        .with_default(ConstantValue::Null),
];

const OPTIONS: &[(&str, &str)] = &[
    ("lifetime", "cookie_lifetime"),
    ("path", "cookie_path"),
    ("domain", "cookie_domain"),
    ("secure", "cookie_secure"),
    ("partitioned", "cookie_partitioned"),
    ("httponly", "cookie_httponly"),
    ("samesite", "cookie_samesite"),
];

pub const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync("session_get_cookie_params", &[], get),
    FunctionEntry::asynchronous("session_set_cookie_params", PARAMETERS, set),
];

fn get(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut params = PhpArray::new();
    for &(key, setting) in OPTIONS {
        let value = match key {
            "lifetime" => Value::Int(settings::integer(context, setting)),
            "secure" | "partitioned" | "httponly" => {
                Value::Bool(settings::boolean(context, setting))
            }
            _ => Value::String(settings::text(context, setting)),
        };
        params.insert(ArrayKey::str(key), value);
    }
    Ok(Value::from(params))
}

fn set(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "session_set_cookie_params";
        if !settings::boolean(context, "use_cookies") {
            context.warning(format!(
                "{function}(): Session cookies cannot be used when session.use_cookies is disabled"
            ));
            return Ok(Value::Bool(false));
        }
        if !can_change(context, function, "cookie parameters", true) {
            return Ok(Value::Bool(false));
        }
        let values = if let Some(options) = args::array(&arguments, 0) {
            read_options(context, &arguments, options).await?
        } else {
            [
                args::int(&arguments, 0).map(|value| value.to_string().into()),
                args::string(&arguments, 1).cloned(),
                args::string(&arguments, 2).cloned(),
                args::bool(&arguments, 3).map(bool_text),
                None,
                args::bool(&arguments, 4).map(bool_text),
                None,
            ]
        };
        for ((_, setting), value) in OPTIONS.iter().zip(values) {
            if let Some(value) = value
                && !settings::update(context, setting, value, function)?
            {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    })
}

async fn read_options(
    context: &mut dyn Context,
    arguments: &[Value],
    options: &PhpArray,
) -> Result<[Option<PhpString>; 7], NativeError> {
    const LABEL: &str = "session_set_cookie_params(): Argument #1 ($lifetime_or_options)";
    for (index, arg) in arguments.iter().enumerate().skip(1) {
        if !matches!(arg, Value::Null) {
            let name = PARAMETERS[index].name;
            return Err(NativeError::value_error(format!(
                "session_set_cookie_params(): Argument #{} (${name}) must be null when argument #1 \
                 ($lifetime_or_options) is an array",
                index + 1
            )));
        }
    }
    let mut values = [const { None }; 7];
    for (key, value) in options.iter() {
        let ArrayKey::String(key) = key else {
            context.warning(format!("{LABEL} cannot contain numeric keys"));
            continue;
        };
        let bytes = key.as_bytes();
        let Some(index) = OPTIONS
            .iter()
            .position(|(name, _)| bytes.eq_ignore_ascii_case(name.as_bytes()))
        else {
            let key = String::from_utf8_lossy(c_string(bytes));
            context.warning(format!("{LABEL} contains an unrecognized key \"{key}\""));
            continue;
        };
        let value = value.try_deref()?.clone();
        values[index] = Some(match OPTIONS[index].0 {
            "secure" | "partitioned" | "httponly" => bool_text(to_bool(&value)),
            _ if value.is_array() => {
                context.warning("Array to string conversion".into());
                "Array".into()
            }
            _ => context.stringify(&value).await?,
        });
    }
    if values.iter().all(Option::is_none) {
        return Err(NativeError::value_error(format!(
            "{LABEL} must contain at least 1 valid key"
        )));
    }
    Ok(values)
}

fn bool_text(value: bool) -> PhpString {
    if value { "1".into() } else { "0".into() }
}
