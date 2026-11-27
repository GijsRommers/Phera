//! The `phera` command: run a PHP script, serve a site, check or start a
//! project, or show the bytecode of a file.

mod arguments;
mod build;
mod check;
mod debug_settings;
mod lint;
mod new;
mod packages;
mod run;
mod serve;
mod upgrade;

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
        Ok(Command::Run {
            debug,
            image,
            script,
            arguments,
            files,
            ini,
        }) => run::script(&script, &arguments, files, ini, debug, image.as_deref()),
        Ok(Command::Build(options)) => build::run(*options),
        Ok(Command::Dump { script }) => run::dump(&script),
        Ok(Command::Serve(options)) => serve::serve(*options),
        Ok(Command::Help(help)) => {
            print!("{help}");
            ExitCode::SUCCESS
        }
        Ok(Command::Check(options)) => check::check(options),
        Ok(Command::Lint(command)) => lint::command(command),
        Ok(Command::New(options)) => new::project(&options),
        Ok(Command::Packages(arguments)) => packages::run(arguments),
        Ok(Command::Upgrade(options)) => upgrade::command(options),
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
