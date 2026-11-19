//! Parse commands and startup options, preserving everything after a script name as PHP argv.

use crate::new::{NewOptions, Template, template_keys};
use phera_dotenv::FileSelection;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) const USAGE: &str = "\
Usage:
  phera [run] [ENV OPTIONS] <file.php> [arguments...]   run a script
  phera serve [OPTIONS] [PATH]            serve PATH (a directory, or one
                                          script that answers every request)
    --listen ADDRESS                      default 127.0.0.1:8080, or
                                          0.0.0.0:$PORT when PORT is set
    --workers N                           default: one per CPU core
    --request-timeout SECONDS             stop a request after this long
                                          (default 60; 0: never)
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
  phera check [--level 0|1|2|3] [--format text|json|github] [--no-progress]
              [--generate-baseline FILE] <paths...>
  phera --version

ENV OPTIONS: --env-file PATH (repeatable) or --no-env-file";

pub(crate) enum Command {
    Run {
        files: FileSelection,
        script: String,
        /// What the script sees as `$argv`: its own name first.
        arguments: Vec<String>,
    },
    Dump {
        script: String,
    },
    Serve(ServeOptions),
    Check(phera_analyzer::config::Options),
    New(crate::new::NewOptions),
    Version,
}

pub(crate) struct ServeOptions {
    pub files: FileSelection,
    /// `None`: `0.0.0.0:$PORT` when a platform sets `PORT`, else
    /// `127.0.0.1:8080`.
    pub listen: Option<SocketAddr>,
    /// `None` means one worker per CPU core.
    pub workers: Option<usize>,
    /// `--compress`: gzip or brotli for clients that accept it. Off unless
    /// given, since the proxy in front usually compresses.
    pub compress: bool,
    /// `--request-timeout`: `None` keeps the server's default, and
    /// `Some(None)` (given as 0) lets a request run without a limit.
    pub request_timeout: Option<Option<Duration>>,
    /// `--trusted-proxies`, not yet parsed into ranges.
    pub trusted_proxies: Option<String>,
    /// `--health`: a path the server answers itself.
    pub health_path: Option<String>,
    pub path: PathBuf,
}

pub(crate) fn parse(arguments: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut arguments = arguments.peekable();
    let mut files = FileSelection::Automatic;
    leading_files(&mut arguments, &mut files)?;
    match arguments.peek().map(String::as_str) {
        None => Err("No script given.".to_string()),
        Some("--version") => Ok(Command::Version),
        Some("--dump") => {
            arguments.next();
            let script = arguments.next().ok_or("--dump needs a file.")?;
            Ok(Command::Dump { script })
        }
        Some("check") => {
            arguments.next();
            phera_analyzer::config::Options::parse(arguments).map(Command::Check)
        }
        Some("serve") => {
            arguments.next();
            parse_serve(arguments, files).map(Command::Serve)
        }
        Some("new") => {
            arguments.next();
            parse_new(arguments).map(Command::New)
        }
        Some(_) => {
            // `phera run file.php` and `phera file.php` mean the same.
            arguments.next_if(|argument| argument == "run");
            leading_files(&mut arguments, &mut files)?;
            let arguments: Vec<String> = arguments.collect();
            let script = arguments.first().cloned().ok_or("No script given.")?;
            Ok(Command::Run {
                script,
                arguments,
                files,
            })
        }
    }
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

fn leading_files(
    arguments: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    files: &mut FileSelection,
) -> Result<(), String> {
    while let Some(option) =
        arguments.next_if(|s| matches!(s.as_str(), "--env-file" | "--no-env-file"))
    {
        file_option(&option, arguments, files)?;
    }
    Ok(())
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
) -> Result<ServeOptions, String> {
    let mut options = ServeOptions {
        files,
        listen: None,
        workers: None,
        compress: false,
        request_timeout: None,
        trusted_proxies: None,
        health_path: None,
        path: PathBuf::from("."),
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
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
