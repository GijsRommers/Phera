//! Opening a gzip wrapper resolves the path before moving file work off the request thread.

use phera_extension_api::{Context, NativeError};

use crate::errors::{nonempty_path, open_failure_message, reason};
use crate::io_policy::IoPolicy;
use crate::mode::Mode;
use crate::request::{directory, files, path, search};
use crate::stream::{Backing, Stream, Wrapper};
use crate::wrapper::Target;

use super::Gzip;

pub(crate) async fn open_gzip(
    context: &mut dyn Context,
    function: &str,
    filename: &[u8],
    uri: &[u8],
    mode: &[u8],
    include_path: bool,
    direct: bool,
) -> Result<Option<Stream>, NativeError> {
    nonempty_path(filename)?;
    let mode = mode.split(|byte| *byte == 0).next().unwrap_or_default();
    if mode.contains(&b'+') {
        context.warning(format!(
            "{function}(): Cannot open a zlib stream for reading and writing at the same time!"
        ));
        return Ok(None);
    }
    let Some(parsed) = Mode::parse(mode) else {
        context.warning_bytes(&open_failure_message(
            function,
            uri,
            format!(
                "`{}' is not a valid mode for fopen",
                String::from_utf8_lossy(mode)
            )
            .as_bytes(),
        ));
        return Ok(None);
    };
    if mode.contains(&b'T') || matches!(mode.first(), Some(b'c' | b'x')) {
        context.warning(format!("{function}(): gzopen failed"));
        return Ok(None);
    }
    let filename = match Target::parse(filename) {
        Target::File(filename) => filename,
        _ => {
            context.warning(format!("{function}(): gzopen failed"));
            return Ok(None);
        }
    };
    let include = include_path.then(|| (context.include_path(), context.script_path()));
    let directory = directory(context);
    let found = include
        .and_then(|(include, script)| search(directory, filename, &include, script.as_deref()));
    let resolved = found.map_or_else(|| directory.resolve(path(filename)), Ok);
    let mask = files(context).mask;
    let level = mode
        .iter()
        .rev()
        .find(|byte| byte.is_ascii_digit())
        .map_or(6, |byte| u32::from(byte - b'0'));
    let opened = IoPolicy::Blocking
        .run(move || {
            let file = parsed.open(&resolved?, mask)?;
            if parsed.readable() {
                Gzip::reader(file)
            } else {
                Ok(Gzip::writer(file, level))
            }
        })
        .await?;
    match opened {
        Ok(gzip) => {
            let wrapper = if direct {
                Wrapper::GzipDirect
            } else {
                Wrapper::Gzip
            };
            Ok(Some(Stream::new(
                Backing::Gzip(Box::new(gzip)),
                wrapper,
                mode,
                uri,
            )))
        }
        Err(error) => {
            context.warning_bytes(&open_failure_message(
                function,
                uri,
                reason(&error).as_bytes(),
            ));
            Ok(None)
        }
    }
}
