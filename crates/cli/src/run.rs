//! `phera file.php` and `phera --dump file.php`.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use phera_dotenv::{Environment, FileSelection};
use phera_runtime::{OutputSink, Runtime, RuntimeError};

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
    ini: Vec<(String, String)>,
    debug: bool,
    image_path: Option<&Path>,
) -> ExitCode {
    let loaded_image = match image_path {
        Some(path) => match phera_runtime::image::open_image(path) {
            Ok(image) => {
                if image.mode() == phera_bytecode::facts::BuildMode::Strict
                    && !image.manifest().has_cli
                {
                    eprintln!(
                        "Image {} has no CLI units; rebuild with `phera build --target cli` or `--target both`.",
                        path.display()
                    );
                    return ExitCode::FAILURE;
                }
                Some(image)
            }
            Err(error) => {
                eprintln!("Cannot open image {}: {error}", path.display());
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    let (resolved_script, resolved_args): (String, Vec<String>) = if let Some(image) = &loaded_image
    {
        if script.is_empty() {
            let entry = image
                .manifest()
                .entries
                .iter()
                .find_map(|rel| image.find_relative(rel).filter(|e| e.unit_cli.is_some()))
                .or_else(|| {
                    (image.mode() == phera_bytecode::facts::BuildMode::Hybrid)
                        .then(|| {
                            image
                                .manifest()
                                .entries
                                .first()
                                .and_then(|rel| image.find_relative(rel))
                        })
                        .flatten()
                });
            let Some(entry) = entry else {
                eprintln!("Image has no runnable CLI entrypoint; pass a script path.");
                return ExitCode::FAILURE;
            };
            let full = entry.canonical_path.clone();
            (full.clone(), vec![full])
        } else {
            let script_path = Path::new(script);
            if script_path.is_file() {
                // Local file exists at the user's working directory: resolve
                // relative to CWD first so we never hijack a local file with an
                // image entry from another directory.
                let canonical = script_path
                    .canonicalize()
                    .unwrap_or_else(|_| script_path.to_path_buf());
                if let Some(entry) = image.find_canonical(&canonical) {
                    if entry.unit_cli.is_some()
                        || image.mode() == phera_bytecode::facts::BuildMode::Hybrid
                    {
                        let mut args = arguments.to_vec();
                        if args.is_empty() {
                            args.push(entry.canonical_path.clone());
                        }
                        (entry.canonical_path.clone(), args)
                    } else {
                        eprintln!(
                            "Image has no CLI unit for `{script}`; rebuild with `--target cli` or `--target both`."
                        );
                        return ExitCode::FAILURE;
                    }
                } else if image.mode() == phera_bytecode::facts::BuildMode::Strict {
                    println!("Could not open input file: {script}");
                    return ExitCode::FAILURE;
                } else {
                    (script.to_owned(), arguments.to_vec())
                }
            } else {
                // Not on disk at CWD (e.g. stripped container): try absolute
                // CWD-joined path first, then explicit project-relative name.
                let cwd_abs = if script_path.is_absolute() {
                    Some(script_path.to_path_buf())
                } else {
                    std::env::current_dir()
                        .ok()
                        .map(|cwd| cwd.join(script_path))
                };
                let entry = cwd_abs
                    .as_deref()
                    .and_then(|p| image.find_canonical(p))
                    .or_else(|| {
                        script_path
                            .is_relative()
                            .then(|| image.find_relative(script))
                            .flatten()
                    });
                match entry {
                    Some(entry) if entry.unit_cli.is_some() => {
                        let mut args = arguments.to_vec();
                        if args.is_empty() {
                            args.push(entry.canonical_path.clone());
                        }
                        (entry.canonical_path.clone(), args)
                    }
                    Some(_) => {
                        eprintln!(
                            "Image has no CLI unit for `{script}`; rebuild with `--target cli` or `--target both`."
                        );
                        return ExitCode::FAILURE;
                    }
                    None => {
                        println!("Could not open input file: {script}");
                        return ExitCode::FAILURE;
                    }
                }
            }
        }
    } else {
        if !Path::new(script).is_file() {
            // PHP's own wording, on standard output as PHP prints it.
            println!("Could not open input file: {script}");
            return ExitCode::FAILURE;
        }
        (script.to_owned(), arguments.to_vec())
    };

    let root = loaded_image
        .as_ref()
        .map(|img| Path::new(&img.manifest().project_root))
        .unwrap_or_else(|| {
            Path::new(&resolved_script)
                .parent()
                .unwrap_or_else(|| Path::new("."))
        });
    let loaded = match files.load(root, Environment::process()) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let debug_enabled = debug;
    let project = crate::debug_settings::project_root(root);
    let settings = crate::debug_settings::read_table(project).and_then(|(table, _)| {
        crate::debug_settings::configure(debug_enabled, &table, &ini, &loaded.environment)
    });
    let debug = match settings {
        Ok(debug) => debug,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    if debug.is_some()
        && loaded_image
            .as_ref()
            .is_some_and(|img| img.mode() == phera_bytecode::facts::BuildMode::Strict)
    {
        eprintln!(
            "Cannot enable PHP step debugging with a strict AOT image (use a hybrid image or omit --image)."
        );
        return ExitCode::FAILURE;
    }
    let mut runtime = Runtime::with_environment(loaded.environment).with_startup_ini(ini.clone());
    if let Some(image) = loaded_image {
        runtime = runtime.with_image(image);
    }
    if let Some(debug_cfg) = debug {
        runtime = runtime.with_debug(debug_cfg);
    }
    match runtime.run_file_blocking(&resolved_script, &resolved_args, Some(Box::new(Stdout))) {
        Ok(result) => {
            if let Some(pending) = result.pending_serve {
                return crate::serve::serve(crate::arguments::ServeOptions::for_script(
                    std::path::PathBuf::from(resolved_script),
                    pending.addr,
                    pending.workers,
                    files,
                    ini,
                    debug_enabled,
                    image_path.map(Path::to_path_buf),
                ));
            }
            if std::env::var_os("PHERA_INSTRUCTION_COUNT").is_some() {
                eprintln!("Executed instructions: {}", result.executed_instructions);
            }
            // Exit statuses are one byte; PHP truncates the same way.
            ExitCode::from(result.status as u8)
        }
        Err(RuntimeError::Compile(error)) => {
            Stdout.write(&runtime.cli_compile_output(error));
            ExitCode::from(255)
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
