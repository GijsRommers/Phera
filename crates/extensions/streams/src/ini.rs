//! `parse_ini_file('app.ini', true)` reads through the same wrappers as file_get_contents.

use phera_ext_standard::ini::{PROCESS_SECTIONS, SCANNER_MODE, parse};
use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Registry, Value, args,
};

use crate::{
    errors::{check_path, open_failure_message},
    handles::FILENAME,
    wrapper::{Target, open},
};

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::asynchronous(
        "parse_ini_file",
        const { &[FILENAME, PROCESS_SECTIONS, SCANNER_MODE] },
        parse_ini_file,
    ));
    registry.describe_function("parse_ini_file", "array|false", Some("standard"));
}

fn parse_ini_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "parse_ini_file";
        let filename = args::string(&arguments, 0).map_or(&[][..], |name| name.as_bytes());
        check_path(function, "#1 ($filename)", filename)?;
        if filename.is_empty() {
            return Err(NativeError::value_error(
                "parse_ini_file(): Argument #1 ($filename) must not be empty",
            ));
        }
        // PHP opens INI files in include mode, whose default rejects URL wrappers.
        if matches!(Target::parse(filename), Target::Data(_)) {
            context.warning(
                concat!(
                    "parse_ini_file(): data:// wrapper is disabled in the server configuration ",
                    "by allow_url_include=0"
                )
                .into(),
            );
            context.warning_bytes(&open_failure_message(
                function,
                filename,
                b"no suitable wrapper could be found",
            ));
            return Ok(Value::Bool(false));
        }
        let Some(mut stream) = open(context, function, filename, b"rb", true).await? else {
            return Ok(Value::Bool(false));
        };
        let input = stream
            .read_policy(None)
            .run(move || stream.read(None))
            .await?;
        match input {
            Ok(bytes) => Ok(parse(
                context,
                &bytes,
                &String::from_utf8_lossy(filename),
                &arguments,
            )),
            Err(_) => Ok(Value::Bool(false)),
        }
    })
}
