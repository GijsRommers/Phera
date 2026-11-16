//! `phera file.php` and `phera --dump file.php`.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use phera_runtime::{OutputSink, Runtime};

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
}

pub(crate) fn script(script: &str, arguments: &[String]) -> ExitCode {
    if !Path::new(script).is_file() {
        // PHP's own wording, on standard output as PHP prints it.
        println!("Could not open input file: {script}");
        return ExitCode::FAILURE;
    }
    match Runtime::new().run_file_blocking(script, arguments, Some(Box::new(Stdout))) {
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

pub(crate) fn dump(script: &str) -> ExitCode {
    match Runtime::new().dump_file_blocking(script) {
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
