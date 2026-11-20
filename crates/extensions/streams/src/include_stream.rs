//! `require 'virtual://script'` reads PHP source from an application wrapper.

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
    let flags = USE_PATH | OPEN_FOR_INCLUDE | 0x0001_0000;
    let Some(stream) = open(context, wrapper, function, path, b"rb", flags, Value::Null).await?
    else {
        return Ok(Some(IncludeSource::Unavailable));
    };
    let name = stream.opened_path.as_deref().unwrap_or(path);
    let result: Result<_, NativeError> = async {
        stream
            .option(context, function, 2, 0, Value::Int(platform_buffer_size()))
            .await?;
        if once
            && context
                .included_files()
                .iter()
                .any(|(_, value)| matches!(value, Value::String(file) if file.as_bytes() == name))
        {
            return Ok(IncludeSource::AlreadyIncluded);
        }
        let bytes = stream.collect(context, function, None).await?;
        let name = String::from_utf8_lossy(name).into_owned();
        Ok(IncludeSource::Source { name, bytes })
    }
    .await;
    let close = stream.close(context).await;
    let result = result?;
    close?;
    Ok(Some(result))
}

pub(crate) fn platform_buffer_size() -> i64 {
    if cfg!(target_os = "macos") {
        1024
    } else {
        8192
    }
}
