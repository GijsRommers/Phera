//! Package command options, e.g. `phera install --offline --no-dev -o`.
//!
//! Package operations live in `phera-packages`; the CLI only reports their outcome.

use phera_packages::Options;
use std::process::ExitCode;

pub(crate) fn run(arguments: Vec<String>) -> ExitCode {
    match execute(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn execute(arguments: &[String]) -> Result<(), String> {
    let mut options = Options::default();
    let mut subject = None;
    let mut development = false;
    for argument in &arguments[1..] {
        match argument.as_str() {
            "--no-dev" => options.no_dev = true,
            "--frozen-lockfile" => options.frozen = true,
            "--offline" => options.offline = true,
            "-o" | "--optimize-autoloader" => options.optimize = true,
            "-v" | "--verbose" => options.verbose = true,
            "--no-scripts" => options.no_scripts = true,
            "--no-install" => options.no_install = true,
            "--prefer-lowest" => options.prefer_lowest = true,
            "--ignore-platform-reqs" => options.ignore_platform = true,
            "--dev" => development = true,
            value if !value.starts_with('-') && subject.is_none() => subject = Some(value),
            other => return Err(format!("Unknown package option: {other}")),
        }
    }
    let directory = std::env::current_dir().map_err(|error| error.to_string())?;
    options.platform = phera_runtime::Runtime::new().platform_packages();
    let result = match arguments[0].as_str() {
        "install" => phera_packages::install(&directory, &options),
        "update" => phera_packages::update(&directory, &options),
        "add" => phera_packages::add(
            &directory,
            subject.ok_or("Usage: phera add vendor/package[:constraint] [--dev]")?,
            development,
            &options,
        ),
        "remove" => phera_packages::remove(
            &directory,
            subject.ok_or("Usage: phera remove vendor/package")?,
            &options,
        ),
        "why" => phera_packages::why(
            &directory,
            subject.ok_or("Usage: phera why vendor/package")?,
        )
        .map(|report| println!("{report}")),
        "outdated" => {
            phera_packages::outdated(&directory, &options).map(|report| println!("{report}"))
        }
        "dump-autoload" => phera_packages::dump_autoload(&directory, &options),
        other => return Err(format!("Unknown package command: {other}")),
    };
    result.map_err(|error| format!("Package operation failed: {error:#}"))
}
