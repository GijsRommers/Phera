//! Startup file selection for `phera run` and `phera serve`.

use std::ffi::OsString;
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

struct Layers {
    process: Environment,
    files: Environment,
    paths: Vec<PathBuf>,
}

impl FileSelection {
    pub fn load(&self, root: &Path, process: Environment) -> Result<Loaded, String> {
        let mut layers = Layers {
            process,
            files: Environment::default(),
            paths: Vec::new(),
        };
        match self {
            Self::Disabled => {}
            Self::Explicit(paths) => {
                for path in paths {
                    layers.read(path, path, false)?;
                }
            }
            Self::Automatic => layers.automatic(root)?,
        }
        layers.process.0.extend(layers.files.0);
        Ok(Loaded {
            environment: layers.process,
            paths: layers.paths,
        })
    }
}

impl Layers {
    fn automatic(&mut self, root: &Path) -> Result<(), String> {
        for name in [".env", ".env.local"] {
            let path = root.join(name);
            self.read(&path, &path, true)?;
        }
        let name = self
            .process
            .get(b"APP_ENV")
            .or_else(|| self.files.get(b"APP_ENV"));
        if let Some(name) = name.filter(|n| !n.is_empty()) {
            let name = profile_suffix(name)?;
            for suffix in ["", ".local"] {
                let mut file = OsString::from(".env.");
                file.push(&name);
                file.push(suffix);
                // The profile name may itself come from a secret file.
                let label = root.join(format!(".env.<APP_ENV>{suffix}"));
                self.read(&root.join(file), &label, true)?;
            }
        }
        Ok(())
    }

    fn read(&mut self, path: &Path, label: &Path, optional: bool) -> Result<(), String> {
        let file = match File::open(path) {
            Err(e) if optional && e.kind() == ErrorKind::NotFound => return Ok(()),
            result => result.map_err(|e| format!("{}: {e}", label.display()))?,
        };
        let mut bytes = Vec::new();
        file.take(FILE_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{}: {e}", label.display()))?;
        self.files = parse(label, &bytes, &self.process, &self.files).map_err(|e| e.to_string())?;
        self.paths.push(
            path.canonicalize()
                .map_err(|e| format!("{}: {e}", label.display()))?,
        );
        Ok(())
    }
}

fn profile_suffix(name: &[u8]) -> Result<OsString, String> {
    if name.contains(&0) || name.contains(&b'/') || (cfg!(windows) && name.contains(&b'\\')) {
        return Err("APP_ENV: expected a filename suffix without a path separator".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(std::ffi::OsStr::from_bytes(name).to_owned())
    }
    #[cfg(not(unix))]
    {
        std::str::from_utf8(name)
            .map(OsString::from)
            .map_err(|_| "APP_ENV: expected a valid filename suffix".into())
    }
}
