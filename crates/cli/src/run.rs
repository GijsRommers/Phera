//! `phera file.php` and `phera --dump file.php`.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use phera_dotenv::{Environment, FileSelection};
use phera_runtime::{OutputSink, Runtime};
use phera_source::LanguageLevel;

/// Writes a script's output to the terminal as it is produced, so a long
/// running script shows progress.
struct Stdout;

impl OutputSink for Stdout {
    fn write(&mut self, bytes: &[u8]) {
        // If the terminal is gone (a closed pipe), there is nobody left to
        // tell, so the error is dropped.
        let mut stdout = io::stdout();
        let _ = stdout.write_all(bytes);
        let _ = stdout.flush();
    }

    fn flush(&mut self) {
        let _ = io::stdout().flush();
    }
}

pub(crate) fn script(
    script: &str,
    arguments: &[String],
    files: FileSelection,
    php: Option<LanguageLevel>,
) -> ExitCode {
    if !Path::new(script).is_file() {
        // PHP's own wording, on standard output as PHP prints it.
        println!("Could not open input file: {script}");
        return ExitCode::FAILURE;
    }
    let root = Path::new(script).parent().unwrap_or_else(|| Path::new("."));
    let Some(level) = resolve_level(php, script) else {
        return ExitCode::from(2);
    };
    let loaded = match files.load(root, Environment::process()) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    match Runtime::with_level_and_environment(level, loaded.environment).run_file_blocking(
        script,
        arguments,
        Some(Box::new(Stdout)),
    ) {
        Ok(result) => {
            if std::env::var_os("PHERA_INSTRUCTION_COUNT").is_some() {
                eprintln!("Executed instructions: {}", result.executed_instructions);
            }
            // Exit statuses are one byte; PHP truncates the same way.
            ExitCode::from(result.status as u8)
        }
        Err(error) => {
            print!("{error}");
            ExitCode::from(255)
        }
    }
}

pub(crate) fn dump(script: &str, php: Option<LanguageLevel>) -> ExitCode {
    let Some(level) = resolve_level(php, script) else {
        return ExitCode::from(2);
    };
    match Runtime::for_level(level).dump_file_blocking(script) {
        Ok(listing) => {
            print!("{listing}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            print!("{error}");
            ExitCode::from(255)
        }
    }
}

fn resolve_level(php: Option<LanguageLevel>, script: &str) -> Option<LanguageLevel> {
    let directory = crate::project_level::script_directory(Path::new(script));
    match crate::project_level::resolve(php, directory) {
        Ok(level) => Some(level),
        Err(error) => {
            eprintln!("{error}");
            None
        }
    }
}
