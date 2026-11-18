//! The `phera` command: run a PHP script, serve a site, or show the bytecode
//! of a file.

mod arguments;
mod check;
mod run;
mod serve;

use std::process::ExitCode;

use mimalloc::MiMalloc;

use arguments::Command;

/// Every allocation in the process goes through mimalloc. Measured on
/// call-heavy and string-heavy PHP code it is 10 to 18 percent faster than
/// the system allocator.
#[global_allocator]
static ALLOCATOR: MiMalloc = MiMalloc;

fn main() -> ExitCode {
    match arguments::parse(std::env::args().skip(1)) {
        Ok(Command::Run { script, arguments }) => run::script(&script, &arguments),
        Ok(Command::Dump { script }) => run::dump(&script),
        Ok(Command::Serve(options)) => serve::serve(options),
        Ok(Command::Check(options)) => check::check(options),
        Ok(Command::Version) => {
            println!("Phera {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}\n\n{}", arguments::USAGE);
            ExitCode::from(2)
        }
    }
}
