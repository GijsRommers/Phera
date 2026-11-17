//! Startup file selection for `phera run` and `phera serve`.

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use crate::{Environment, parse, parser::FILE_LIMIT};

#[derive(Default)]
pub enum FileSelection {
    #[default]
    Automatic,
    Explicit(Vec<PathBuf>),
    Disabled,
}

/// Includes canonical paths so HTTP static-file serving can exclude loaded files.
pub struct Loaded {
    pub environment: Environment,
    pub paths: Vec<PathBuf>,
}

impl FileSelection {
    pub fn load(&self, root: &Path, mut process: Environment) -> Result<Loaded, String> {
        let mut files = Environment::default();
        let mut paths = Vec::new();
        let mut load = |path: &Path, label: &Path, optional, previous: &Environment| {
            let file = match File::open(path) {
                Err(e) if optional && e.kind() == ErrorKind::NotFound => return Ok(None),
                result => result.map_err(|e| format!("{}: {e}", label.display()))?,
            };
            let mut bytes = Vec::new();
            file.take(FILE_LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| format!("{}: {e}", label.display()))?;
            let parsed = parse(label, &bytes, &process, previous).map_err(|e| e.to_string())?;
            paths.push(
                path.canonicalize()
                    .map_err(|e| format!("{}: {e}", label.display()))?,
            );
            Ok::<_, String>(Some(parsed))
        };
        match self {
            Self::Disabled => {}
            Self::Explicit(selected) => {
                for path in selected {
                    if let Some(parsed) = load(path, path, false, &files)? {
                        files = parsed;
                    }
                }
            }
            Self::Automatic => {
                for name in [".env", ".env.local"] {
                    let path = root.join(name);
                    if let Some(parsed) = load(&path, &path, true, &files)? {
                        files = parsed;
                    }
                }
                let name = process.get(b"APP_ENV").or_else(|| files.get(b"APP_ENV"));
                if let Some(name) = name.filter(|n| !n.is_empty()) {
                    if !name
                        .iter()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
                    {
                        return Err("APP_ENV: expected a filename suffix using A-Z, a-z, 0-9, '.', '_' or '-'".into());
                    }
                    let name = std::str::from_utf8(name)
                        .expect("validated ASCII")
                        .to_owned();
                    for suffix in ["", ".local"] {
                        let path = root.join(format!(".env.{name}{suffix}"));
                        // APP_ENV may itself come from a secret file.
                        let label = root.join(format!(".env.<APP_ENV>{suffix}"));
                        if let Some(parsed) = load(&path, &label, true, &files)? {
                            files = parsed;
                        }
                    }
                }
            }
        }
        process.0.extend(files.0);
        Ok(Loaded {
            environment: process,
            paths,
        })
    }
}
