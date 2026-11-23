//! `phera check src/`: report static mistakes without loading or executing PHP.

use phera_analyzer::{
    baseline,
    config::Options,
    diagnostic::{self, Format},
};
use std::io::IsTerminal;
use std::process::ExitCode;

pub(crate) const HELP: &str = "\
Usage: phera check [OPTIONS] [paths...]

Type-check PHP. Paths may also be set in [check].paths in phera.toml.

Options:
  --level LEVEL             Choose level 0, 1, 2 or 3 (default: 1).
  --format FORMAT           Print text, json or github output.
  --no-progress             Hide progress output.
  --generate-baseline FILE  Save a baseline. Load it through [check].baseline.
  -h, --help                Print this help.
  --                        Treat the remaining arguments as paths.

Exit codes: 0 clean, 1 findings, 2 command error.
";

pub(crate) fn check(options: Options) -> ExitCode {
    match run(&options) {
        Ok(found) => ExitCode::from(u8::from(found)),
        Err(error) => {
            eprintln!("phera check: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(options: &Options) -> Result<bool, String> {
    let mut report = phera_analyzer::check(options)?;
    if let Some(path) = &options.generate_baseline {
        baseline::write(path, &report.diagnostics)?;
    } else if let Some(path) = &options.baseline {
        baseline::filter(path, &mut report.diagnostics)?;
    }
    print!("{}", diagnostic::render(&report, options.format));
    if !options.no_progress && std::io::stderr().is_terminal() {
        eprintln!(
            "Parsed {} vendor files for declarations.",
            report.vendor_files
        );
    }
    if matches!(options.format, Format::Text) {
        let affected = report.diagnostics.chunk_by(|a, b| a.file == b.file).count();
        println!(
            "{} finding{} in {} of {} file{}, level {}, {:.2} s",
            report.diagnostics.len(),
            if report.diagnostics.len() == 1 {
                ""
            } else {
                "s"
            },
            affected,
            report.files,
            if report.files == 1 { "" } else { "s" },
            options.level,
            report.elapsed.as_secs_f64(),
        );
    }
    Ok(!report.diagnostics.is_empty())
}
