//! Parse commands and startup options, preserving everything after a script name as PHP argv.

use phera_dotenv::FileSelection;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) const USAGE: &str = "\
Usage:
  phera [run] [ENV OPTIONS] <file.php> [arguments...]   run a script
  phera serve [ENV OPTIONS] [--listen ADDRESS] [--workers N] [--compress]
              [--request-timeout SECONDS] [PATH]
                                          serve PATH (a directory, or one
                                          script that answers every request);
                                          --compress sends gzip or brotli to
                                          clients that accept it;
                                          a request is stopped after 60
                                          seconds unless --request-timeout
                                          says otherwise (0: never)
  phera --dump <file.php>                 print the compiled bytecode
  phera check [--level 0|1] [--format text|json|github] [--no-progress]
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
    Version,
}

pub(crate) struct ServeOptions {
    pub files: FileSelection,
    pub listen: SocketAddr,
    /// `None` means one worker per CPU core.
    pub workers: Option<usize>,
    /// `--compress`: gzip or brotli for clients that accept it. Off unless
    /// given, since the proxy in front usually compresses.
    pub compress: bool,
    /// `--request-timeout`: `None` keeps the server's default, and
    /// `Some(None)` (given as 0) lets a request run without a limit.
    pub request_timeout: Option<Option<Duration>>,
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
        listen: SocketAddr::from(([127, 0, 0, 1], 8080)),
        workers: None,
        compress: false,
        request_timeout: None,
        path: PathBuf::from("."),
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--env-file" | "--no-env-file" => {
                file_option(&argument, &mut arguments, &mut options.files)?
            }
            "--listen" => {
                let value = arguments.next().ok_or("--listen needs an address.")?;
                options.listen = value
                    .parse()
                    .map_err(|_| format!("Not an address like 127.0.0.1:8080: {value}"))?;
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
