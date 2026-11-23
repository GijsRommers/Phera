//! `require 'virtual://script'` reads PHP source from an application wrapper.

use std::borrow::Cow;

use phera_extension_api::{Context, NativeError, Value};

use crate::errors::open_failure_message;
use crate::user_file::{OPEN_FOR_INCLUDE, open};
use crate::user_wrappers::{USE_PATH, file_disabled, locate};
use crate::wrapper::Target;

pub enum IncludeSource {
    Unavailable,
    AlreadyIncluded,
    Source { name: String, bytes: Vec<u8> },
}

pub fn wraps_include(context: &mut dyn Context, path: &[u8]) -> bool {
    locate(context, path).is_some()
        || file_disabled(context) && matches!(Target::parse(path), Target::File(_))
}

pub async fn include_source(
    context: &mut dyn Context,
    path: &[u8],
    function: &str,
    once: bool,
) -> Result<Option<IncludeSource>, NativeError> {
    let Some(wrapper) = locate(context, path) else {
        if file_disabled(context) && matches!(Target::parse(path), Target::File(_)) {
            context.warning(format!(
                "{function}(): file:// wrapper is disabled in the server configuration"
            ));
            context.warning_bytes(&open_failure_message(
                function,
                path,
                b"no suitable wrapper could be found",
            ));
            return Ok(Some(IncludeSource::Unavailable));
        }
        return Ok(None);
    };
    let (open_path, use_path) = resolved_path(context, path);
    let flags = use_path | OPEN_FOR_INCLUDE | 0x0001_0000;
    let Some(stream) = open(
        context,
        wrapper,
        function,
        &open_path,
        b"rb",
        flags,
        Value::Null,
    )
    .await?
    else {
        return Ok(Some(IncludeSource::Unavailable));
    };
    let name = stream.opened_path.as_deref().unwrap_or(&open_path);
    let result: Result<_, NativeError> = async {
        stream
            .option(context, function, 2, 0, Value::Int(platform_buffer_size()))
            .await?;
        if once
            && context
                .included_files()
                .iter()
                .any(|(_, value)| value.as_str_bytes() == Some(name))
        {
            return Ok(IncludeSource::AlreadyIncluded);
        }
        let length = match stream.stat(context, function).await? {
            Some(Value::Array(fields)) => crate::user_stat::fields(context, &fields)?[7],
            _ => 0,
        };
        let length = u64::try_from(length).ok().filter(|length| *length > 0);
        let bytes = stream
            .collect_bytes(context, function, length, false)
            .await?;
        let name = String::from_utf8_lossy(name).into_owned();
        Ok(IncludeSource::Source { name, bytes })
    }
    .await;
    let close = stream.close(context).await;
    let result = result?;
    close?;
    Ok(Some(result))
}

fn resolved_path<'a>(context: &mut dyn Context, path: &'a [u8]) -> (Cow<'a, [u8]>, i64) {
    if let Target::File(filename) = Target::parse(path) {
        let include_path = context.include_path();
        let script = context.script_path();
        if let Some(resolved) = crate::request::search(
            crate::request::directory(context),
            filename,
            &include_path,
            script.as_deref(),
        ) {
            return (Cow::Owned(crate::request::bytes(&resolved).to_vec()), 0);
        }
    }
    (Cow::Borrowed(path), USE_PATH)
}

pub(crate) fn platform_buffer_size() -> i64 {
    if cfg!(target_os = "macos") {
        1024
    } else {
        8192
    }
}
