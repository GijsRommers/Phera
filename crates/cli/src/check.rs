//! `phera check src/`: report static mistakes without loading or executing PHP.

use phera_analyzer::{
    baseline,
    config::Options,
    diagnostic::{self, Format},
};
use std::io::IsTerminal;
use std::process::ExitCode;

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
    if matches!(options.format, Format::Text) {
        let affected = report.diagnostics.chunk_by(|a, b| a.file == b.file).count();
        println!(
            "{} findings in {} of {} files, level {}, {:.2} s",
            report.diagnostics.len(),
            affected,
            report.files,
            options.level,
            report.elapsed.as_secs_f64(),
        );
    }
    if !options.no_progress && std::io::stderr().is_terminal() {
        eprintln!(
            "Parsed {} vendor files for declarations.",
            report.vendor_files
        );
    }
    Ok(!report.diagnostics.is_empty())
}
