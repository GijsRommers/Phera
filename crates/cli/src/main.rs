//! The `phera` command: run a PHP script, serve a site, check or start a
//! project, or show the bytecode of a file.

mod arguments;
mod check;
mod composer_level;
mod new;
mod project_level;
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
        Ok(Command::Run {
            script,
            arguments,
            files,
            php,
        }) => run::script(&script, &arguments, files, php),
        Ok(Command::Dump { script, php }) => run::dump(&script, php),
        Ok(Command::Serve(options)) => serve::serve(options),
        Ok(Command::Check(options)) => check::check(options),
        Ok(Command::New(options)) => new::project(&options),
        Ok(Command::Version) => {
            let [older, newer] = phera_source::LanguageLevel::ALL;
            println!(
                "Phera {}, PHP {} and {}",
                env!("CARGO_PKG_VERSION"),
                older.release(),
                newer.release(),
            );
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}\n\n{}", arguments::USAGE);
            ExitCode::from(2)
        }
    }
}
