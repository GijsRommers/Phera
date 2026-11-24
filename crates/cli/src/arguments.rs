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
    --preload                             compile Composer files before listening
    --no-class-warmup                     do not declare in the snapshot the
                                          classes requests autoload
    --no-service-warmup                   Laravel, Symfony: do not resolve the
                                          framework's services before the
                                          snapshot
    --warmup PATH                         Laravel, Symfony: handle a GET of
                                          PATH before the snapshot (may be
                                          repeated; see the README)
    --no-process                          forbid starting external programs
    --request-timeout SECONDS             stop a request after this long
                                          (default 60; 0: never)
    --request-read-timeout SECONDS        receive headers and body in this
                                          many seconds (default 60), plus
                                          one second per 16 KiB of body read
                                          so large uploads can keep going
    --dev                                 show PHP diagnostics in responses
                                          and check PHP files each request
    -d NAME=VALUE                         set a startup PHP INI value
    --compress                            gzip or brotli for clients that
                                          accept it
    --trusted-proxies LIST                believe X-Forwarded-* from these
                                          addresses or ranges (10.0.0.0/8),
                                          separated by commas; the word
                                          private means every private range
    --health PATH                         answer this path with 200 and ok,
                                          without running PHP
    --ws-max-connections N                maximum open WebSocket connections
                                          per worker (default 5000)
    --ws-max-message-size BYTES           maximum WebSocket message size in
                                          bytes (default 1048576)
    --ws-idle-timeout SECONDS             close idle WebSocket connections after
                                          this many seconds (default 90; 0: off)
    --ws-ping-interval SECONDS            send WebSocket pings every N seconds
                                          (default 30; 0: off)
    --ws-send-queue N                     maximum outbound queued WebSocket
                                          messages per connection (default 64)
    --ws-allowed-origins LIST             comma-separated WebSocket origins
                                          allowed (* for any; default same
                                          origin)
    ENV OPTIONS as below
  phera new [OPTIONS] [DIRECTORY]         start a project; asks for what
                                          is not given
    --template NAME                       plain, symfony, symfony-webapp or
                                          laravel
    --no-install                          do not fetch packages
  phera --dump <file.php>                 print the compiled bytecode
  phera check [OPTIONS] <paths...>        type-check PHP (see check --help)
  phera lint [OPTIONS] [paths...]         lint PHP (see lint --help)
  phera install [--no-dev] [--frozen-lockfile] [--offline] [-o]
  phera update [packages...] [-W] [--dry-run] [--lock]
  phera add vendor/package[:constraint]... [--dev]
  phera remove vendor/package...
  phera audit [--locked] [--no-dev] [--format table|json]
  phera why vendor/package
  phera why-not vendor/package version
  phera outdated [--all]
  phera show [vendor/package]
  phera licenses
  phera validate
  phera run-script name [arguments...]
  phera exec binary [arguments...]
  phera dump-autoload [-o] [--no-dev]
  phera upgrade [OPTIONS]                 update Phera (see upgrade --help)
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
    Serve(Box<ServeOptions>),
    Packages(Vec<String>),
    Help(&'static str),
    Check(phera_analyzer::config::Options),
    Lint(phera_analyzer::lint::Command),
    New(crate::new::NewOptions),
    Upgrade(crate::upgrade::Options),
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
    pub preload: bool,
    /// Declare in the snapshot the classes requests autoload; on unless
    /// `--no-class-warmup` is given.
    pub class_warmup: bool,
    /// Let the framework adapters resolve services before the snapshot; on
    /// unless `--no-service-warmup` is given.
    pub service_warmup: bool,
    /// `--warmup`: the paths the framework adapters handle as requests
    /// before the snapshot, in the order given.
    pub warmup: Vec<String>,
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
    pub ws_max_connections: Option<usize>,
    pub ws_max_message_size: Option<usize>,
    pub ws_idle_timeout: Option<Duration>,
    pub ws_ping_interval: Option<Duration>,
    pub ws_send_queue: Option<usize>,
    pub ws_allowed_origins: Option<Vec<String>>,
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
            command_with_help(arguments, crate::check::HELP, |args| {
                phera_analyzer::config::Options::parse(args).map(Command::Check)
            })
        }
        Some("lint") => {
            arguments.next();
            command_with_help(arguments, crate::lint::HELP, |args| {
                phera_analyzer::lint::Options::parse(args).map(Command::Lint)
            })
        }
        Some("serve") => {
            arguments.next();
            if matches!(arguments.peek().map(String::as_str), Some("--help" | "-h")) {
                return Ok(Command::Help(USAGE));
            }
            parse_serve(arguments, files, ini)
                .map(Box::new)
                .map(Command::Serve)
        }
        Some("new") => {
            arguments.next();
            parse_new(arguments).map(Command::New)
        }
        Some(
            "install" | "update" | "add" | "require" | "remove" | "audit" | "why" | "why-not"
            | "outdated" | "show" | "licenses" | "validate" | "run-script" | "exec"
            | "dump-autoload",
        ) => Ok(Command::Packages(arguments.collect())),
        Some("upgrade") => {
            arguments.next();
            command_with_help(arguments, crate::upgrade::HELP, |args| {
                crate::upgrade::Options::parse(args).map(Command::Upgrade)
            })
        }
        Some(_) => {
            let named = arguments.next_if(|argument| argument == "run").is_some();
            if named
                && matches!(&files, FileSelection::Automatic)
                && ini.is_empty()
                && arguments
                    .peek()
                    .is_some_and(|name| crate::packages::is_script(name))
            {
                let args = std::iter::once("run-script".to_owned())
                    .chain(arguments)
                    .collect();
                return Ok(Command::Packages(args));
            }
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

fn command_with_help(
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
        preload: false,
        class_warmup: true,
        service_warmup: true,
        warmup: Vec::new(),
        compress: false,
        no_process: false,
        request_timeout: None,
        request_read_timeout: None,
        trusted_proxies: None,
        health_path: None,
        ws_max_connections: None,
        ws_max_message_size: None,
        ws_idle_timeout: None,
        ws_ping_interval: None,
        ws_send_queue: None,
        ws_allowed_origins: None,
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
            "--preload" => options.preload = true,
            "--no-class-warmup" => options.class_warmup = false,
            "--no-service-warmup" => options.service_warmup = false,
            "--warmup" => {
                let path = arguments
                    .next()
                    .filter(|path| is_warmup_path(path))
                    .ok_or("--warmup needs a path like /api/posts?page=1, without spaces.")?;
                options.warmup.push(path);
            }
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
            "--ws-max-connections" => {
                options.ws_max_connections = Some(parse_positive_usize(
                    &mut arguments,
                    "--ws-max-connections",
                )?);
            }
            "--ws-max-message-size" => {
                options.ws_max_message_size = Some(parse_positive_usize(
                    &mut arguments,
                    "--ws-max-message-size",
                )?);
            }
            "--ws-idle-timeout" => {
                options.ws_idle_timeout =
                    Some(parse_seconds_duration(&mut arguments, "--ws-idle-timeout")?);
            }
            "--ws-ping-interval" => {
                options.ws_ping_interval = Some(parse_seconds_duration(
                    &mut arguments,
                    "--ws-ping-interval",
                )?);
            }
            "--ws-send-queue" => {
                options.ws_send_queue =
                    Some(parse_positive_usize(&mut arguments, "--ws-send-queue")?);
            }
            "--ws-allowed-origins" => {
                let list = arguments
                    .next()
                    .ok_or("--ws-allowed-origins needs a comma-separated list of origins.")?;
                let origins: Vec<String> = list
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                if origins.is_empty() {
                    return Err("--ws-allowed-origins needs one or more origins.".to_string());
                }
                options.ws_allowed_origins = Some(origins);
            }
            option if option.starts_with("--") => {
                return Err(format!("Unknown option: {option}"));
            }
            path => options.path = PathBuf::from(path),
        }
    }
    Ok(options)
}

fn parse_positive_usize(
    arguments: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<usize, String> {
    let value = arguments
        .next()
        .ok_or_else(|| format!("{flag} needs a positive integer."))?;
    value
        .parse()
        .ok()
        .filter(|&n| n > 0)
        .ok_or_else(|| format!("Not a positive integer for {flag}: {value}"))
}

fn parse_seconds_duration(
    arguments: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<Duration, String> {
    let value = arguments
        .next()
        .ok_or_else(|| format!("{flag} needs a number of seconds."))?;
    let seconds: u64 = value
        .parse()
        .map_err(|_| format!("Not a non-negative number of seconds for {flag}: {value}"))?;
    Ok(Duration::from_secs(seconds))
}

/// Whether `path` can be a warm-up request: a path on this site, in one
/// piece (the adapters receive the paths separated by white space).
pub(crate) fn is_warmup_path(path: &str) -> bool {
    path.starts_with('/') && !path.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_serve_websocket_flags() {
        let cmd = parse(
            [
                "serve",
                "--ws-max-connections",
                "2000",
                "--ws-max-message-size",
                "65536",
                "--ws-idle-timeout",
                "0",
                "--ws-ping-interval",
                "20",
                "--ws-send-queue",
                "16",
                "--ws-allowed-origins",
                "https://a.test, https://b.test",
                "public",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .expect("parse serve");
        let Command::Serve(options) = cmd else {
            panic!("expected Serve");
        };
        assert_eq!(options.ws_max_connections, Some(2000));
        assert_eq!(options.ws_max_message_size, Some(65536));
        assert_eq!(options.ws_idle_timeout, Some(Duration::ZERO));
        assert_eq!(options.ws_ping_interval, Some(Duration::from_secs(20)));
        assert_eq!(options.ws_send_queue, Some(16));
        assert_eq!(
            options.ws_allowed_origins.as_deref(),
            Some(["https://a.test".to_owned(), "https://b.test".to_owned()].as_slice())
        );
    }
}
