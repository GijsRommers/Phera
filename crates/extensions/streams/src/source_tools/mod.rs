//! highlight_file('example.php', true) returns colored HTML without executing the source.
//! php_strip_whitespace('example.php') returns the same source with trivia collapsed.

mod highlight;
mod iptc;
mod meta_tags;
mod strip;

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Registry, Value, args,
};
use phera_lexer::Lexer;

use crate::{
    errors::{check_path, open_failure_message, reason},
    handles::FILENAME,
    wrapper::{Target, open},
};

const RETURN: Param =
    Param::new("return", ParamType::Bool).with_default(ConstantValue::Bool(false));

pub(crate) fn register(registry: &mut Registry) {
    iptc::register(registry);
    meta_tags::register(registry);
    registry.function(FunctionEntry::sync(
        "highlight_string",
        const { &[Param::new("string", ParamType::String), RETURN] },
        highlight_string,
    ));
    registry.function(FunctionEntry::asynchronous(
        "highlight_file",
        const { &[FILENAME, RETURN] },
        highlight_file,
    ));
    registry.function(FunctionEntry::asynchronous(
        "show_source",
        const { &[FILENAME, RETURN] },
        show_source,
    ));
    registry.function(FunctionEntry::asynchronous(
        "php_strip_whitespace",
        &[FILENAME],
        strip_file,
    ));
    for (name, returns) in [
        ("highlight_string", "string|true"),
        ("highlight_file", "string|bool"),
        ("show_source", "string|bool"),
        ("php_strip_whitespace", "string"),
    ] {
        registry.describe_function(name, returns, Some("standard"));
    }
}

fn highlight_string(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let source = args::bytes(arguments, 0);
    Ok(highlight_result(context, arguments, source))
}

fn highlight_file(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(highlight_path(context, arguments, "highlight_file"))
}

fn show_source(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(highlight_path(context, arguments, "show_source"))
}

async fn highlight_path(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    let path = args::bytes(&arguments, 0);
    check_path(function, "#1 ($filename)", path)?;
    let input = read_source(context, function, path).await;
    match input {
        Ok(Some(source)) => Ok(highlight_result(context, &arguments, &source)),
        result => {
            context.warning_bytes(
                &[
                    function.as_bytes(),
                    b"(): Failed opening '",
                    path,
                    b"' for highlighting",
                ]
                .concat(),
            );
            result.map(|_| Value::Bool(false))
        }
    }
}

fn highlight_result(context: &mut dyn Context, arguments: &[Value], source: &[u8]) -> Value {
    let html = highlight::render(context, source, lexer(context, source));
    if args::bool(arguments, 1).unwrap_or(false) {
        Value::from(html)
    } else {
        context.write(&html);
        Value::Bool(true)
    }
}

fn strip_file(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let path = args::bytes(&arguments, 0);
        let function = "php_strip_whitespace";
        check_path(function, "#1 ($filename)", path)?;
        let Some(source) = read_source(context, function, path).await? else {
            return Ok(Value::from(""));
        };
        Ok(Value::from(strip::source(&source, lexer(context, &source))))
    })
}

fn lexer<'a>(context: &dyn Context, source: &'a [u8]) -> Lexer<'a> {
    Lexer::for_tokenizer(source).with_short_open_tags(ini_enabled(context, b"short_open_tag"))
}

fn ini_enabled(context: &dyn Context, name: &[u8]) -> bool {
    context
        .ini_get(name)
        .is_some_and(|setting| phera_extension_api::ini::boolean(setting.as_bytes()))
}

async fn read_source(
    context: &mut dyn Context,
    function: &str,
    path: &[u8],
) -> Result<Option<Vec<u8>>, NativeError> {
    if matches!(Target::parse(path), Target::Data(_)) && !ini_enabled(context, b"allow_url_include")
    {
        context.warning(format!(
            "{function}(): data:// wrapper is disabled in the server configuration by allow_url_include=0"
        ));
        context.warning_bytes(&open_failure_message(
            function,
            path,
            b"no suitable wrapper could be found",
        ));
        return Ok(None);
    }
    let Some(mut stream) = open(context, function, path, b"rb", true).await? else {
        return Ok(None);
    };
    match stream
        .read_policy(None)
        .run(move || stream.read(None))
        .await?
    {
        Ok(input) => Ok(Some(input)),
        Err(error) => {
            let detail = match error.kind() {
                std::io::ErrorKind::IsADirectory => "Undefined error: 0".into(),
                _ => reason(&error),
            };
            context.warning_bytes(&open_failure_message(function, path, detail.as_bytes()));
            Ok(None)
        }
    }
}
