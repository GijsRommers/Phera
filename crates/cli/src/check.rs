//! `phera check src/`: report static mistakes without loading or executing PHP.

use phera_analyzer::{baseline, config::Options, diagnostic};
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
    print!(
        "{}",
        diagnostic::render(&report.diagnostics, options.format)
    );
    if !options.no_progress && std::io::stderr().is_terminal() {
        eprintln!(
            "Checked {} files in {:.3}s ({:.0} files/s).",
            report.files,
            report.elapsed.as_secs_f64(),
            report.files as f64 / report.elapsed.as_secs_f64()
        );
    }
    Ok(!report.diagnostics.is_empty())
}
