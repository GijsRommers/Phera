//! `phera lint src --fix` checks PHP source and applies verified edits.

use phera_analyzer::{
    diagnostic::Format,
    lint::{self, Command},
};
use std::process::ExitCode;

pub(crate) const HELP: &str = "\
Usage: phera lint [OPTIONS] [paths...]

Check PHP 8.5 source. With no paths, search the project and skip vendor/.
Pass a path inside vendor/ to inspect it explicitly.

Options:
  --fix                     Apply safe fixes.
  --fix-unsafe              Also apply fixes that can change behavior.
  --rule NAME               Select a rule (repeatable).
  --only GROUP              Select a group (repeatable), or use --only all.
  --all                     Select every rule and group.
  --skip NAME               Exclude a rule (repeatable).
  --format FORMAT           Print text, json or github output.
  --threads N               Limit workers (default: all available cores).
  --generate-baseline FILE  Save a baseline. Load it through [lint].baseline.
  --list-rules              List the rule names and descriptions.
  --explain NAME            Show a rule's examples and fix contract.
  -h, --help                Print this help.
  --                        Treat the remaining arguments as paths.

Groups: syntax, suspicious, unused, modernize, style, naming, complexity.
Style, naming and complexity are opt-in. See [lint] in phera.toml.
Exit codes: 0 clean, 1 findings, 2 command error.
";

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
