//! Reading the command line. Written by hand: the grammar is four lines.

use std::net::SocketAddr;
use std::path::PathBuf;

pub(crate) const USAGE: &str = "\
Usage:
  phera [run] <file.php> [arguments...]   run a script
  phera serve [--listen ADDRESS] [--workers N] [PATH]
                                          serve PATH (a directory, or one
                                          script that answers every request)
  phera --dump <file.php>                 print the compiled bytecode
  phera --version";

pub(crate) enum Command {
    Run {
        script: String,
        /// What the script sees as `$argv`: its own name first.
        arguments: Vec<String>,
    },
    Dump {
        script: String,
    },
    Serve(ServeOptions),
    Version,
}

pub(crate) struct ServeOptions {
    pub listen: SocketAddr,
    /// `None` means one worker per CPU core.
    pub workers: Option<usize>,
    pub path: PathBuf,
}

pub(crate) fn parse(arguments: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut arguments = arguments.peekable();
    match arguments.peek().map(String::as_str) {
        None => Err("No script given.".to_string()),
        Some("--version") => Ok(Command::Version),
        Some("--dump") => {
            arguments.next();
            let script = arguments.next().ok_or("--dump needs a file.")?;
            Ok(Command::Dump { script })
        }
        Some("serve") => {
            arguments.next();
            parse_serve(arguments).map(Command::Serve)
        }
        Some(_) => {
            // `phera run file.php` and `phera file.php` mean the same.
            arguments.next_if(|argument| argument == "run");
            let arguments: Vec<String> = arguments.collect();
            let script = arguments.first().cloned().ok_or("No script given.")?;
            Ok(Command::Run { script, arguments })
        }
    }
}

fn parse_serve(mut arguments: impl Iterator<Item = String>) -> Result<ServeOptions, String> {
    let mut options = ServeOptions {
        listen: SocketAddr::from(([127, 0, 0, 1], 8080)),
        workers: None,
        path: PathBuf::from("."),
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
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
            option if option.starts_with("--") => {
                return Err(format!("Unknown option: {option}"));
            }
            path => options.path = PathBuf::from(path),
        }
    }
    Ok(options)
}
