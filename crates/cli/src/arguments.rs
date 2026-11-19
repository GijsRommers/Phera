//! Parse commands and startup options, preserving everything after a script name as PHP argv.

use crate::new::{NewOptions, Template, template_keys};
use phera_dotenv::FileSelection;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) const USAGE: &str = "\
Usage:
  phera [run] [ENV OPTIONS] [-d NAME=VALUE] <file.php> [arguments...]   run a script
  phera serve [OPTIONS] [PATH]            serve PATH (a directory, or one
                                          script that answers every request)
    --listen ADDRESS                      default 127.0.0.1:8080, or
                                          0.0.0.0:$PORT when PORT is set
    --workers N                           default: one per CPU core
    --no-process                          forbid starting external programs
    --request-timeout SECONDS             stop a request after this long
                                          (default 60; 0: never)
    --request-read-timeout SECONDS        receive headers and body in this
                                          many seconds (default 60), plus
                                          one second per 16 KiB of body read
                                          so large uploads can keep going
    --dev                                 show PHP diagnostics in responses
                                          (off by default, logged to stderr)
    -d NAME=VALUE                         set a startup PHP INI value
    --compress                            gzip or brotli for clients that
                                          accept it
    --trusted-proxies LIST                believe X-Forwarded-* from these
                                          addresses or ranges (10.0.0.0/8),
                                          separated by commas; the word
                                          private means every private range
    --health PATH                         answer this path with 200 and ok,
                                          without running PHP
    ENV OPTIONS as below
  phera new [OPTIONS] [DIRECTORY]         start a project; asks for what
                                          is not given
    --template NAME                       plain, symfony, symfony-webapp or
                                          laravel
    --no-install                          do not fetch packages
  phera --dump <file.php>                 print the compiled bytecode
  phera check [OPTIONS] <paths...>        type-check PHP (see check --help)
  phera lint [OPTIONS] [paths...]         lint PHP (see lint --help)
  phera --version

ENV OPTIONS: --env-file PATH (repeatable) or --no-env-file";

pub(crate) enum Command {
    Run {
        files: FileSelection,
        ini: Vec<(String, String)>,
        script: String,
        /// What the script sees as `$argv`: its own name first.
        arguments: Vec<String>,
    },
    Dump {
        script: String,
    },
    Serve(ServeOptions),
    Help(&'static str),
    Check(phera_analyzer::config::Options),
    Lint(phera_analyzer::lint::Command),
    New(crate::new::NewOptions),
    Version,
}

pub(crate) struct ServeOptions {
    pub files: FileSelection,
    pub ini: Vec<(String, String)>,
    pub dev: bool,
    /// `None`: `0.0.0.0:$PORT` when a platform sets `PORT`, else
    /// `127.0.0.1:8080`.
    pub listen: Option<SocketAddr>,
    /// `None` means one worker per CPU core.
    pub workers: Option<usize>,
    /// `--compress`: gzip or brotli for clients that accept it. Off unless
    /// given, since the proxy in front usually compresses.
    pub compress: bool,
    pub no_process: bool,
    /// `--request-timeout`: `None` keeps the server's default, and
    /// `Some(None)` (given as 0) lets a request run without a limit.
    pub request_timeout: Option<Option<Duration>>,
    pub request_read_timeout: Option<Duration>,
    /// `--trusted-proxies`, not yet parsed into ranges.
    pub trusted_proxies: Option<String>,
    /// `--health`: a path the server answers itself.
    pub health_path: Option<String>,
    pub path: PathBuf,
}

pub(crate) fn parse(arguments: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut arguments = arguments.peekable();
    let mut files = FileSelection::Automatic;
    let mut ini = Vec::new();
    leading_options(&mut arguments, &mut files, &mut ini)?;
    match arguments.peek().map(String::as_str) {
        None => Err("No script given.".to_string()),
        Some("--version") => Ok(Command::Version),
        Some("--help" | "-h") => Ok(Command::Help(USAGE)),
        Some("--dump") => {
            arguments.next();
            let script = arguments.next().ok_or("--dump needs a file.")?;
            Ok(Command::Dump { script })
        }
        Some("check") => {
            arguments.next();
            analyzer_command(arguments, crate::check::HELP, |args| {
                phera_analyzer::config::Options::parse(args).map(Command::Check)
            })
        }
        Some("lint") => {
            arguments.next();
            analyzer_command(arguments, crate::lint::HELP, |args| {
                phera_analyzer::lint::Options::parse(args).map(Command::Lint)
            })
        }
        Some("serve") => {
            arguments.next();
            if matches!(arguments.peek().map(String::as_str), Some("--help" | "-h")) {
                return Ok(Command::Help(USAGE));
            }
            parse_serve(arguments, files, ini).map(Command::Serve)
        }
        Some("new") => {
            arguments.next();
            parse_new(arguments).map(Command::New)
        }
        Some(_) => {
            // `phera run file.php` and `phera file.php` mean the same.
            arguments.next_if(|argument| argument == "run");
            leading_options(&mut arguments, &mut files, &mut ini)?;
            let arguments: Vec<String> = arguments.collect();
            let script = arguments.first().cloned().ok_or("No script given.")?;
            Ok(Command::Run {
                script,
                arguments,
                files,
                ini,
            })
        }
    }
}

fn analyzer_command(
    arguments: impl Iterator<Item = String>,
    help: &'static str,
    parse: impl FnOnce(std::vec::IntoIter<String>) -> Result<Command, String>,
) -> Result<Command, String> {
    let arguments: Vec<_> = arguments.collect();
    if arguments
        .iter()
        .take_while(|arg| *arg != "--")
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return Ok(Command::Help(help));
    }
    parse(arguments.into_iter())
}

fn parse_new(mut arguments: impl Iterator<Item = String>) -> Result<NewOptions, String> {
    let mut options = NewOptions {
        directory: None,
        template: None,
        install: true,
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--no-install" => options.install = false,
            "--template" => {
                let key = arguments.next().ok_or("--template needs a name.")?;
                let template = Template::from_key(&key).ok_or_else(|| {
                    format!("Unknown template: {key}. Known: {}.", template_keys())
                })?;
                options.template = Some(template);
            }
            option if option.starts_with('-') => return Err(format!("Unknown option: {option}")),
            _ if options.directory.is_some() => {
                return Err("phera new takes one directory.".to_string());
            }
            _ => options.directory = Some(PathBuf::from(argument)),
        }
    }
    Ok(options)
}

fn leading_options(
    arguments: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    files: &mut FileSelection,
    ini: &mut Vec<(String, String)>,
) -> Result<(), String> {
    while let Some(option) =
        arguments.next_if(|s| matches!(s.as_str(), "--env-file" | "--no-env-file" | "-d"))
    {
        if option == "-d" {
            ini.push(ini_option(arguments)?);
        } else {
            file_option(&option, arguments, files)?;
        }
    }
    Ok(())
}

fn ini_option(arguments: &mut impl Iterator<Item = String>) -> Result<(String, String), String> {
    let setting = arguments.next().ok_or("-d needs a name=value setting.")?;
    let (name, value) = setting.split_once('=').unwrap_or((&setting, "1"));
    let name = name.trim();
    if name.is_empty() {
        return Err("-d needs a setting name.".into());
    }
    Ok((name.to_owned(), value.trim().to_owned()))
}

fn file_option(
    option: &str,
    arguments: &mut impl Iterator<Item = String>,
    files: &mut FileSelection,
) -> Result<(), String> {
    match (option, &mut *files) {
        ("--no-env-file", FileSelection::Disabled | FileSelection::Automatic) => {
            *files = FileSelection::Disabled
        }
        ("--env-file", mode @ FileSelection::Automatic) => {
            *mode = FileSelection::Explicit(vec![
                arguments.next().ok_or("--env-file needs a path.")?.into(),
            ]);
        }
        ("--env-file", FileSelection::Explicit(paths)) => {
            paths.push(arguments.next().ok_or("--env-file needs a path.")?.into())
        }
        _ => return Err("Use either --env-file or --no-env-file.".into()),
    }
    Ok(())
}

fn parse_serve(
    mut arguments: impl Iterator<Item = String>,
    files: FileSelection,
    ini: Vec<(String, String)>,
) -> Result<ServeOptions, String> {
    let mut options = ServeOptions {
        files,
        ini,
        dev: false,
        listen: None,
        workers: None,
        compress: false,
        no_process: false,
        request_timeout: None,
        request_read_timeout: None,
        trusted_proxies: None,
        health_path: None,
        path: PathBuf::from("."),
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-d" => options.ini.push(ini_option(&mut arguments)?),
            "--env-file" | "--no-env-file" => {
                file_option(&argument, &mut arguments, &mut options.files)?
            }
            "--listen" => {
                let value = arguments.next().ok_or("--listen needs an address.")?;
                let address = value
                    .parse()
                    .map_err(|_| format!("Not an address like 127.0.0.1:8080: {value}"))?;
                options.listen = Some(address);
            }
            "--workers" => {
                let value = arguments.next().ok_or("--workers needs a number.")?;
                let workers: usize = value
                    .parse()
                    .ok()
                    .filter(|&count| count > 0)
                    .ok_or_else(|| format!("Not a number of workers: {value}"))?;
                options.workers = Some(workers);
            }
            "--compress" => options.compress = true,
            "--dev" => options.dev = true,
            "--no-process" => options.no_process = true,
            "--trusted-proxies" => {
                let list = arguments
                    .next()
                    .ok_or("--trusted-proxies needs addresses or ranges.")?;
                options.trusted_proxies = Some(list);
            }
            "--health" => {
                let path = arguments
                    .next()
                    .filter(|path| path.starts_with('/'))
                    .ok_or("--health needs a path like /healthz.")?;
                options.health_path = Some(path);
            }
            "--request-read-timeout" => {
                let value = arguments
                    .next()
                    .ok_or("--request-read-timeout needs a number of seconds.")?;
                let seconds: u64 = value
                    .parse()
                    .ok()
                    .filter(|&seconds| seconds > 0)
                    .ok_or_else(|| format!("Not a positive number of seconds: {value}"))?;
                options.request_read_timeout = Some(Duration::from_secs(seconds));
            }
            "--request-timeout" => {
                let value = arguments
                    .next()
                    .ok_or("--request-timeout needs a number of seconds.")?;
                let seconds: u64 = value
                    .parse()
                    .map_err(|_| format!("Not a number of seconds: {value}"))?;
                options.request_timeout = Some((seconds > 0).then(|| Duration::from_secs(seconds)));
            }
            option if option.starts_with("--") => {
                return Err(format!("Unknown option: {option}"));
            }
            path => options.path = PathBuf::from(path),
        }
    }
    Ok(options)
}
