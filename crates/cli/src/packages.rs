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
  --no-audit                     skip the Packagist advisory summary
  -o, --optimize-autoloader       build a class map from PSR mappings
  -a, --classmap-authoritative    look up classes only in the class map
  --ignore-platform-req PACKAGE   skip a named platform requirement
  --ignore-platform-reqs         skip all platform requirements
  -v, --verbose                  show package installation details
";

const UPDATE_HELP: &str = "\
Usage: phera update [packages...] [OPTIONS]

Resolve dependencies within composer.json constraints and refresh composer.lock.

  -W, --with-all-dependencies    update dependencies of the named packages
  --dry-run                      print version changes without applying them
  --lock                         refresh only the existing lock's content hash
  --no-install                   write the lock without installing packages
  --no-audit                     skip the Packagist advisory summary
  --offline                      use the local package cache
  --prefer-lowest                prefer the lowest allowed versions
";

const AUDIT_HELP: &str = "\
Usage: phera audit [OPTIONS]

Check packages against Packagist security advisories.

  --locked                       audit composer.lock instead of vendor
  --no-dev                       omit development dependencies
  --format table|json            choose the output format (default: table)
";

pub(crate) fn run(arguments: Vec<String>) -> ExitCode {
    match execute(&arguments) {
        Ok(status) => status,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn execute(arguments: &[String]) -> Result<ExitCode, String> {
    let command = arguments[0].as_str();
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "--help" | "-h"))
    {
        print!("{}", help(command));
        return Ok(ExitCode::SUCCESS);
    }
    let mut options = Options {
        audit_summary: matches!(command, "install" | "update" | "add" | "require" | "remove"),
        ..Options::default()
    };
    let mut subjects = Vec::new();
    let mut development = false;
    let mut format = "table";
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
            "--no-audit" => options.audit_summary = false,
            "--format" if command == "audit" => {
                format = remaining.next().ok_or("--format needs table or json.")?;
            }
            value if command == "audit" && value.starts_with("--format=") => {
                format = &value["--format=".len()..];
            }
            "--no-install" => options.no_install = true,
            "--dry-run" if matches!(command, "update" | "add" | "require" | "remove") => {
                options.dry_run = true;
            }
            "--lock" if command == "update" => options.refresh_lock = true,
            "-W" | "--with-all-dependencies"
                if matches!(command, "update" | "add" | "require" | "remove") =>
            {
                options.with_all_dependencies = true;
            }
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
            value if !value.starts_with('-') => subjects.push(value.to_owned()),
            other => return Err(format!("Unknown package option: {other}")),
        }
    }
    let directory = std::env::current_dir().map_err(|error| error.to_string())?;
    options.platform = phera_runtime::Runtime::new().platform_packages();
    if !matches!(command, "update" | "add" | "require" | "remove" | "why") && !subjects.is_empty() {
        return Err(format!("Unexpected positional argument for {command}."));
    }
    if command == "audit" {
        if !matches!(format, "table" | "json") {
            return Err("--format needs table or json.".into());
        }
        let report = phera_packages::audit(&directory, &options)
            .map_err(|error| format!("Audit failed: {error:#}"))?;
        let output = if format == "json" {
            report.json()
        } else {
            report.table()
        }
        .map_err(|error| format!("Audit output failed: {error:#}"))?;
        println!("{output}");
        return Ok(ExitCode::from(u8::from(report.has_advisories())));
    }
    let result = match command {
        "install" => phera_packages::install(&directory, &options),
        "update" => {
            let subjects: Vec<_> = subjects
                .iter()
                .map(|name| name.to_ascii_lowercase())
                .collect();
            phera_packages::update_selected(&directory, &subjects, &options)
        }
        "add" | "require" => phera_packages::add_many(&directory, &subjects, development, &options),
        "remove" => phera_packages::remove_many(&directory, &subjects, &options),
        "why" => {
            let [subject] = subjects.as_slice() else {
                return Err("Usage: phera why vendor/package".into());
            };
            phera_packages::why(&directory, subject).map(|report| println!("{report}"))
        }
        "outdated" => {
            phera_packages::outdated(&directory, &options).map(|report| println!("{report}"))
        }
        "dump-autoload" => phera_packages::dump_autoload(&directory, &options),
        other => return Err(format!("Unknown package command: {other}")),
    };
    result
        .map(|()| ExitCode::SUCCESS)
        .map_err(|error| format!("Package operation failed: {error:#}"))
}

fn help(command: &str) -> &'static str {
    match command {
        "install" => INSTALL_HELP,
        "update" => UPDATE_HELP,
        "audit" => AUDIT_HELP,
        "add" | "require" => {
            "Usage: phera add [--dev] vendor/package[:constraint]...\n\n\
            Add production dependencies, or development dependencies with --dev.\n\
            The Composer spelling, `phera require`, is also accepted.\n"
        }
        "remove" => {
            "Usage: phera remove vendor/package...\n\n\
            Remove root dependencies and update the lock.\n"
        }
        "why" => "Usage: phera why vendor/package\n",
        "outdated" => "Usage: phera outdated\n",
        "dump-autoload" => "Usage: phera dump-autoload [-o] [-a] [--no-dev]\n",
        _ => crate::arguments::USAGE,
    }
}
