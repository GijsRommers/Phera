//! Run a package binary from vendor/bin with the configured project working directory.

use crate::{Project, paths};
use anyhow::{Context, Result, bail};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path};
use std::process::{Command, ExitStatus};

pub fn exec(directory: &Path, name: &str, arguments: &[String]) -> Result<ExitStatus> {
    let project = Project::read(directory)?;
    let mut name_components = Path::new(name).components();
    if !matches!(name_components.next(), Some(Component::Normal(_)))
        || name_components.next().is_some()
    {
        bail!("Pass a binary name in vendor/bin.");
    }
    paths::relative(name)?;
    let bin = project.bin_dir()?;
    let executable = bin.join(name);
    let target = executable
        .canonicalize()
        .with_context(|| format!("{name} is missing. Run `phera install` first."))?;
    if !target.starts_with(&project.directory) || !target.is_file() {
        bail!("{name} must point to a file inside the project directory.");
    }
    let mut bytes = Vec::new();
    File::open(&target)?.take(512).read_to_end(&mut bytes)?;
    let source = String::from_utf8_lossy(&bytes);
    let source = source.trim_start_matches('\u{feff}').trim_start();
    let php = if let Some(shebang) = source.strip_prefix("#!") {
        shebang.lines().next().is_some_and(|line| {
            line.split_whitespace().any(|word| {
                let file = word.rsplit('/').next().unwrap_or(word);
                file.strip_prefix("php").is_some_and(|suffix| {
                    suffix
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || byte == b'.')
                })
            })
        })
    } else {
        source.starts_with("<?php")
    };
    let mut command = if php {
        let mut phera = Command::new(std::env::current_exe()?);
        phera.arg(&executable);
        phera
    } else {
        Command::new(executable)
    };
    let mut path = vec![bin];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    Ok(command
        .current_dir(&project.directory)
        .env("PATH", std::env::join_paths(path)?)
        .args(arguments)
        .status()?)
}
