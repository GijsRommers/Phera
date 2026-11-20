//! Root Composer event scripts, e.g. `"post-install-cmd": ["@php setup.php"]`.
//!
//! Shell scripts inherit the project directory; PHP commands use this Phera executable.

use crate::{Options, Project, php, project::strings};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::io::Write;
use std::process::Command;

#[derive(Default)]
pub(crate) struct Runner {
    environment: BTreeMap<String, String>,
}

impl Runner {
    pub(crate) fn event(&mut self, project: &Project, options: &Options, name: &str) -> Result<()> {
        if options.no_scripts {
            return Ok(());
        }
        self.environment
            .entry("COMPOSER_DEV_MODE".into())
            .or_insert_with(|| if options.no_dev { "0" } else { "1" }.into());
        for script in strings(&project.data["scripts"][name]) {
            run(project, options, name, script, &mut self.environment, 0).with_context(|| {
                format!("Script for {name} failed. Fix it or use --no-scripts.")
            })?;
        }
        Ok(())
    }
}

fn run(
    project: &Project,
    options: &Options,
    event: &str,
    script: &str,
    environment: &mut BTreeMap<String, String>,
    depth: usize,
) -> Result<()> {
    if depth > 32 {
        bail!("Composer script aliases form a cycle (limit 32).");
    }
    let binary = std::env::current_exe().context("Cannot locate the Phera executable.")?;
    if let Some(assignment) = script.strip_prefix("@putenv ") {
        let (key, value) = assignment
            .split_once('=')
            .context("@putenv expects KEY=value.")?;
        environment.insert(key.into(), value.into());
        return Ok(());
    }
    let mut temporary = None;
    let command = if let Some(php_script) = script.strip_prefix("@php ") {
        format!("{} {php_script}", shell_quote(&binary.to_string_lossy()))
    } else if let Some(arguments) = script.strip_prefix("@composer ") {
        format!("{} {arguments}", shell_quote(&binary.to_string_lossy()))
    } else if let Some(alias) = script.strip_prefix('@') {
        let scripts = &project.data["scripts"][alias];
        if scripts.is_null() {
            bail!("Unknown Composer script alias @{alias}.");
        }
        for nested in strings(scripts) {
            run(project, options, alias, nested, environment, depth + 1)?;
        }
        return Ok(());
    } else if script.contains("::") && !script.contains(char::is_whitespace) {
        let mut file = tempfile::Builder::new()
            .prefix(".phera-script-")
            .suffix(".php")
            .tempfile_in(&project.directory)?;
        let autoload = project.vendor_dir()?.join("autoload.php");
        write!(
            file,
            "<?php\n{}\nrequire {};\n\
             $event = new \\Composer\\Script\\Event({}, {}, {});\n\
             $result = call_user_func({}, $event);\n\
             if (is_int($result)) {{ exit($result); }}\n",
            include_str!("../resources/script_event.php.inc"),
            php::quote(&autoload.to_string_lossy()),
            php::quote(event),
            php::quote(&project.vendor_dir()?.to_string_lossy()),
            !options.no_dev,
            php::quote(script)
        )?;
        let command = format!(
            "{} {}",
            shell_quote(&binary.to_string_lossy()),
            shell_quote(&file.path().to_string_lossy())
        );
        temporary = Some(file);
        command
    } else {
        script.to_owned()
    };
    if options.verbose {
        eprintln!("Running project script.");
    }
    let mut path = vec![project.bin_dir()?];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let status = shell(command)
        .current_dir(&project.directory)
        .envs(environment.iter())
        .env("PATH", std::env::join_paths(path)?)
        .status()?;
    drop(temporary);
    if !status.success() {
        bail!("Project script exited with {status}.");
    }
    Ok(())
}

fn shell(command: String) -> Command {
    let mut shell = if cfg!(windows) {
        Command::new("cmd")
    } else {
        Command::new("sh")
    };
    shell
        .arg(if cfg!(windows) { "/C" } else { "-c" })
        .arg(command);
    shell
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
