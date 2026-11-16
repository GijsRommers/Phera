//! The `phera` command: run a PHP script, serve a site, or show the bytecode
//! of a file.

mod arguments;
mod run;
mod serve;

use std::process::ExitCode;

use arguments::Command;

fn main() -> ExitCode {
    match arguments::parse(std::env::args().skip(1)) {
        Ok(Command::Run { script, arguments }) => run::script(&script, &arguments),
        Ok(Command::Dump { script }) => run::dump(&script),
        Ok(Command::Serve(options)) => serve::serve(options),
        Ok(Command::Version) => {
            println!("Phera {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}\n\n{}", arguments::USAGE);
            ExitCode::FAILURE
        }
    }
}
