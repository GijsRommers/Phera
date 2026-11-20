//! Package command options, e.g. `phera install --offline --no-dev -o`.
//!
//! Package operations live in `phera-packages`; the CLI only reports their outcome.

use phera_packages::Options;
use std::process::ExitCode;

const INSTALL_HELP: &str = "\
Usage: phera install [OPTIONS]

Install packages from composer.lock, or resolve composer.json and write a lock.

  --no-dev                       omit development dependencies and autoload rules
  --frozen-lockfile, --locked     require a current lock
  --offline                      use the local package cache
  --prefer-dist                  prefer distribution archives (default)
  --no-scripts                   skip composer.json scripts
  --no-plugins                   skip native Composer plugin hooks
  -o, --optimize-autoloader       build a class map from PSR mappings
  -a, --classmap-authoritative    look up classes only in the class map
  --ignore-platform-req PACKAGE   skip a named platform requirement
  --ignore-platform-reqs         skip all platform requirements
  -v, --verbose                  show package installation details
";

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
    let command = arguments[0].as_str();
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "--help" | "-h"))
    {
        print!("{}", help(command));
        return Ok(());
    }
    let mut options = Options::default();
    let mut subject = None;
    let mut development = false;
    let mut remaining = arguments[1..].iter();
    while let Some(argument) = remaining.next() {
        match argument.as_str() {
            "--no-dev" => options.no_dev = true,
            "--frozen-lockfile" | "--locked" => options.frozen = true,
            "--offline" => options.offline = true,
            "--prefer-dist" => {}
            "-o" | "--optimize-autoloader" => options.optimize = true,
            "-a" | "--classmap-authoritative" => options.authoritative = true,
            "-v" | "--verbose" => options.verbose = true,
            "--no-scripts" => options.no_scripts = true,
            "--no-plugins" => options.no_plugins = true,
            "--no-install" => options.no_install = true,
            "--prefer-lowest" => options.prefer_lowest = true,
            "--ignore-platform-reqs" => options.ignore_platform = true,
            "--ignore-platform-req" => {
                let name = remaining
                    .next()
                    .filter(|value| !value.starts_with('-'))
                    .ok_or("--ignore-platform-req needs a platform package name.")?;
                options
                    .ignore_platform_requirement(name)
                    .map_err(|error| error.to_string())?;
            }
            value if value.starts_with("--ignore-platform-req=") => {
                let name = &value["--ignore-platform-req=".len()..];
                options
                    .ignore_platform_requirement(name)
                    .map_err(|error| error.to_string())?;
            }
            "--dev" => development = true,
            value if !value.starts_with('-') && subject.is_none() => subject = Some(value),
            other => return Err(format!("Unknown package option: {other}")),
        }
    }
    let directory = std::env::current_dir().map_err(|error| error.to_string())?;
    options.platform = phera_runtime::Runtime::new().platform_packages();
    let result = match command {
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

fn help(command: &str) -> &'static str {
    match command {
        "install" => INSTALL_HELP,
        "update" => "Usage: phera update [--no-install] [--offline] [--prefer-lowest]\n",
        "add" => "Usage: phera add vendor/package[:constraint] [--dev]\n",
        "remove" => "Usage: phera remove vendor/package\n",
        "why" => "Usage: phera why vendor/package\n",
        "outdated" => "Usage: phera outdated\n",
        "dump-autoload" => "Usage: phera dump-autoload [-o] [-a] [--no-dev]\n",
        _ => crate::arguments::USAGE,
    }
}
