//! `curl_setopt_array($handle, [CURLOPT_URL => $url])` stops at the first failure.

mod connection;
mod settings;
mod strings;
mod tls;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Value, args, value::to_bool,
};

use crate::constants::ids::*;
use crate::easy::{Handle, handle};
use crate::error::TransferError;

pub(crate) fn set_option(
    context: &mut dyn Context,
    values: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let handle = handle(&values)?;
        let option = args::int(&values, 1).unwrap_or_default();
        apply(
            context,
            &handle,
            option,
            args::value(&values, 2),
            "curl_setopt",
        )
        .await
    })
}

pub(crate) fn set_array(
    context: &mut dyn Context,
    values: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let handle = handle(&values)?;
        if let Some(array) = args::array(&values, 1) {
            for (key, value) in array.iter() {
                let ArrayKey::Int(option) = key else {
                    return Err(NativeError::value_error(concat!(
                        "curl_setopt_array(): Argument #2 ($options) ",
                        "contains an invalid cURL option"
                    )));
                };
                if !to_bool(&apply(context, &handle, option, value, "curl_setopt_array").await?) {
                    return Ok(Value::Bool(false));
                }
            }
        }
        Ok(Value::Bool(true))
    })
}

async fn apply(
    context: &mut dyn Context,
    handle: &Handle,
    option: i64,
    value: &Value,
    function: &str,
) -> NativeResult {
    let value = value.deref().clone();
    let name = crate::constants::option_name(option);
    if name.is_none() && option != CURLINFO_HEADER_OUT {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #2 (${}) {}",
            if function == "curl_setopt" {
                "option"
            } else {
                "options"
            },
            if function == "curl_setopt" {
                "is not a valid cURL option"
            } else {
                "contains an invalid cURL option"
            },
        )));
    }

    // Conversion can call PHP. Publish only this option on the latest handle state.
    let mut text = None;
    let mut list = Vec::new();
    let is_blob = tls::is_blob(option);
    if (is_blob || strings::takes_text(option) || tls::takes_text(option))
        && !(option == CURLOPT_POSTFIELDS && matches!(value, Value::Array(_)))
    {
        if !matches!(value, Value::Null) || option == CURLOPT_POSTFIELDS {
            let converted = context.stringify(&value).await?;
            if !is_blob && option != CURLOPT_POSTFIELDS && converted.as_bytes().contains(&0) {
                return Err(NativeError::value_error(format!(
                    "{function}(): cURL option must not contain any null bytes"
                )));
            }
            text = Some(converted.as_bytes().to_vec());
        }
    } else if matches!(
        option,
        CURLOPT_HTTPHEADER | CURLOPT_RESOLVE | CURLOPT_CONNECT_TO
    ) {
        let Value::Array(values) = &value else {
            return Err(NativeError::type_error(format!(
                "{function}(): The {} option must have an array value",
                name.unwrap_or_default()
            )));
        };
        for item in values.values() {
            let converted = context.stringify(item).await?;
            if converted.as_bytes().contains(&0) {
                return Err(NativeError::value_error(format!(
                    "{function}(): cURL option must not contain any null bytes"
                )));
            }
            list.push(converted.as_bytes().to_vec());
        }
    }

    if option == CURLOPT_SHARE {
        match &value {
            Value::Object(object)
                if matches!(
                    object.class.name(),
                    "CurlShareHandle" | "CurlSharePersistentHandle"
                ) =>
            {
                let share = crate::classes::state::<crate::share::ShareHandle>(object)?;
                handle.borrow_mut().attach_share(Some(share));
            }
            Value::Null => handle.borrow_mut().attach_share(None),
            _ => {}
        }
        return Ok(Value::Bool(true));
    }
    let (cookies, jar, files, session, command) = {
        let mut easy = handle.borrow_mut();
        match settings::change(
            context,
            &mut easy.settings,
            option,
            &value,
            text,
            list,
            function,
        )? {
            Change::Applied => {
                easy.error = None;
                if option == CURLOPT_URL {
                    easy.info.url = None;
                }
                if option == CURLOPT_CUSTOMREQUEST {
                    easy.info.method = None;
                }
                return Ok(Value::Bool(true));
            }
            Change::Cookie(command) => (
                easy.cookies(),
                easy.settings.cookie_jar.clone(),
                easy.settings.cookie_files.clone(),
                easy.settings.cookie_session,
                command,
            ),
            Change::Refused(reason) => {
                let message = format!("{}: {reason}", name.unwrap_or("CURLINFO_HEADER_OUT"));
                easy.error = Some(TransferError::new(4, &message));
                context.warning(format!("{function}(): {message}"));
                return Ok(Value::Bool(false));
            }
        }
    };
    crate::cookies::command(context, &cookies, jar.as_deref(), &files, session, &command).await;
    Ok(Value::Bool(true))
}

pub(super) enum Change {
    Applied,
    Cookie(Vec<u8>),
    Refused(&'static str),
}
