use std::{
    io::{self, Write},
    process::ExitCode,
};
struct Stdout;
impl phera_runtime::OutputSink for Stdout {
    fn write(&mut self, b: &[u8]) {
        let _ = io::stdout().write_all(b);
        let _ = io::stdout().flush();
    }
}
fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--version") {
        println!("Phera {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.first().is_some_and(|a| a == "run") {
        args.remove(0);
    }
    let dump = args.first().is_some_and(|a| a == "--dump");
    if dump {
        args.remove(0);
    }
    let Some(file) = args.first() else {
        eprintln!("Usage: phera [run|--dump] file.php [args...]");
        return ExitCode::FAILURE;
    };
    if !std::path::Path::new(file).is_file() {
        println!("Could not open input file: {file}");
        return ExitCode::FAILURE;
    }
    let runtime = phera_runtime::Runtime::new();
    if dump {
        match runtime.dump_file_blocking(file) {
            Ok(text) => {
                print!("{text}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                print!("{e}");
                ExitCode::from(255)
            }
        }
    } else {
        match runtime.run_file_blocking(file, &args, Some(Box::new(Stdout))) {
            Ok(result) => ExitCode::from(result.status as u8),
            Err(e) => {
                print!("{e}");
                ExitCode::from(255)
            }
        }
    }
}
