//! `phera lint src --fix` checks PHP source and applies verified edits.

use phera_analyzer::{
    diagnostic::Format,
    lint::{self, Command},
};
use std::process::ExitCode;

pub(crate) fn command(command: Command) -> ExitCode {
    match command {
        Command::Explain(rule) => print!("{}", rule.documentation()),
        Command::List => print!("{}", lint::catalog()),
        Command::Run(options) => {
            let report = match lint::run(&options) {
                Ok(report) => report,
                Err(error) => {
                    eprintln!("phera lint: {error}");
                    return ExitCode::from(2);
                }
            };
            print!("{}", lint::render(&report, options.format));
            if matches!(options.format, Format::Text) {
                println!(
                    "{} findings in {} files, {:.3} s",
                    report.diagnostics.len(),
                    report.files,
                    report.elapsed.as_secs_f64(),
                );
            }
            return ExitCode::from(u8::from(!report.diagnostics.is_empty()));
        }
    }
    ExitCode::SUCCESS
}
