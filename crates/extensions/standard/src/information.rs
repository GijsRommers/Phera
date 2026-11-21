//! `phpinfo(INFO_GENERAL)` and `phpcredits()` print PHP's plain-text report layout.

use std::fmt::Write;

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry,
    Value, args,
};

const ALL: i64 = 0xffff_ffff;
const INFO_GENERAL: i64 = 1;
const INFO_CREDITS: i64 = 2;
const INFO_CONFIGURATION: i64 = 4;
const INFO_MODULES: i64 = 8;
const INFO_ENVIRONMENT: i64 = 16;
const INFO_VARIABLES: i64 = 32;
const INFO_LICENSE: i64 = 64;

const CREDITS: &[(&str, i64, &str, &str)] = &[
    (
        "CREDITS_GROUP",
        1,
        "PHP Group",
        "The PHP Group and PHP contributors",
    ),
    (
        "CREDITS_GENERAL",
        2,
        "Language Design & Concept",
        "The PHP language project",
    ),
    (
        "CREDITS_SAPI",
        4,
        "SAPI Modules",
        "Phera CLI and HTTP server",
    ),
    (
        "CREDITS_MODULES",
        8,
        "Module Authors",
        "Phera extension contributors",
    ),
    (
        "CREDITS_DOCS",
        16,
        "Documentation",
        "PHP Documentation Group and Phera contributors",
    ),
    (
        "CREDITS_QA",
        64,
        "PHP Quality Assurance Team",
        "PHP and Phera test contributors",
    ),
];

pub(super) fn register(registry: &mut Registry) {
    const FLAGS: &[Param] =
        &[Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(ALL))];
    for (name, flags) in [
        ("INFO_GENERAL", INFO_GENERAL),
        ("INFO_CREDITS", INFO_CREDITS),
        ("INFO_CONFIGURATION", INFO_CONFIGURATION),
        ("INFO_MODULES", INFO_MODULES),
        ("INFO_ENVIRONMENT", INFO_ENVIRONMENT),
        ("INFO_VARIABLES", INFO_VARIABLES),
        ("INFO_LICENSE", INFO_LICENSE),
        ("INFO_ALL", ALL),
        ("CREDITS_FULLPAGE", 32),
        ("CREDITS_ALL", ALL),
    ] {
        registry.constant(name, flags);
    }
    for &(name, flag, _, _) in CREDITS {
        registry.constant(name, flag);
    }
    registry.function(FunctionEntry::asynchronous("phpinfo", FLAGS, phpinfo));
    registry.function(FunctionEntry::sync("phpcredits", FLAGS, phpcredits));
    for name in ["phpinfo", "phpcredits"] {
        registry.describe_function(name, "true", Some("standard"));
    }
}

fn phpinfo(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let flags = args::int(&arguments, 0).unwrap_or(ALL);
        let mut output = String::new();
        if flags & INFO_GENERAL != 0 {
            general(context, &mut output);
        }
        if flags & INFO_CREDITS != 0 {
            credits(&mut output, ALL);
        }
        if flags & INFO_CONFIGURATION != 0 {
            output.push_str("\nConfiguration\n\nDirective => Local Value\n");
            for name in [
                "display_errors",
                "error_reporting",
                "html_errors",
                "zend.assertions",
            ] {
                if let Some(value) = context.ini_get(name.as_bytes()) {
                    let _ = writeln!(output, "{name} => {}", value.to_string_lossy());
                }
            }
        }
        if flags & INFO_MODULES != 0 {
            let modules = context
                .call(&Value::from("get_loaded_extensions"), Vec::new())
                .await?;
            output.push_str("\nAdditional Modules\n\nModule Name\n");
            if let Value::Array(modules) = modules {
                for value in modules.values() {
                    if let Value::String(name) = value {
                        let _ = writeln!(output, "{}", name.to_string_lossy());
                    }
                }
            }
        }
        // Request variables can contain headers and secrets just as process
        // environment variables can. The report omits the values of both.
        if flags & INFO_ENVIRONMENT != 0 {
            output.push_str("\nEnvironment\n\nVariable => [redacted]\n");
        }
        if flags & INFO_VARIABLES != 0 {
            output.push_str("\nPHP Variables\n\nVariable => [redacted]\n");
        }
        if flags & INFO_LICENSE != 0 {
            let _ = writeln!(
                output,
                "\nPHP License\n\nPhera is available under {}.",
                env!("CARGO_PKG_LICENSE")
            );
        }
        context.write(output.as_bytes());
        Ok(Value::Bool(true))
    })
}

fn general(context: &mut dyn Context, output: &mut String) {
    let version = context
        .named_constant("PHP_VERSION")
        .and_then(|value| match value {
            Value::String(version) => Some(version),
            _ => None,
        });
    if let Some(version) = version {
        let _ = writeln!(output, "PHP Version => {}", version.to_string_lossy());
    }
    let _ = writeln!(
        output,
        "\nSystem => {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let _ = writeln!(output, "PHP Integer Size => {} bits", i64::BITS);
    output.push_str("Server API => Command Line Interface\n");
}

fn phpcredits(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut output = String::new();
    credits(&mut output, args::int(arguments, 0).unwrap_or(ALL));
    context.write(output.as_bytes());
    Ok(Value::Bool(true))
}

fn credits(output: &mut String, flags: i64) {
    output.push_str("PHP Credits\n");
    for &(_, flag, heading, text) in CREDITS {
        if flags & flag != 0 {
            let _ = writeln!(output, "\n{heading}\n{text}");
        }
    }
}
