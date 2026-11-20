//! Root scripts such as `"post-install-cmd": ["@php setup.php"]`.
//!
//! Shell commands use the project directory and vendor/bin on PATH.

use crate::{Lock, Options, Project, paths, php, plugins, project::strings};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::io::Write;
mod shell;

use shell::{forward, shell, shell_arguments, shell_quote, split_words};

#[derive(Default)]
pub(crate) struct Runner {
    environment: BTreeMap<String, Option<String>>,
}

impl Runner {
    pub(crate) fn event(&mut self, project: &Project, options: &Options, name: &str) -> Result<()> {
        self.with_args(project, options, name, &[])
    }

    pub(crate) fn with_args(
        &mut self,
        project: &Project,
        options: &Options,
        name: &str,
        args: &[String],
    ) -> Result<()> {
        if options.no_scripts {
            return Ok(());
        }
        self.environment
            .entry("COMPOSER_DEV_MODE".into())
            .or_insert_with(|| Some(if options.no_dev { "0" } else { "1" }.into()));
        self.dispatch(project, options, name, args, 0)
            .with_context(|| format!("Script for {name} failed. Fix it or use --no-scripts."))
    }

    fn dispatch(
        &mut self,
        project: &Project,
        options: &Options,
        name: &str,
        args: &[String],
        depth: usize,
    ) -> Result<()> {
        if depth > 32 {
            bail!("Composer script aliases form a cycle (limit 32).");
        }
        if std::env::var("COMPOSER_SKIP_SCRIPTS")
            .is_ok_and(|skipped| skipped.split(',').any(|event| event.trim() == name))
        {
            return Ok(());
        }
        let scripts = &project.data["scripts"][name];
        if name == "auto-scripts" && scripts.is_object() {
            return self.flex(project, options, scripts, args, depth);
        }
        for script in strings(scripts) {
            self.run(project, options, name, script, args, depth)?;
        }
        Ok(())
    }

    fn run(
        &mut self,
        project: &Project,
        options: &Options,
        event: &str,
        script: &str,
        args: &[String],
        depth: usize,
    ) -> Result<()> {
        let script = script.trim();
        if let Some(assignment) = script.strip_prefix("@putenv ") {
            let (key, value) = assignment
                .split_once('=')
                .map_or((assignment, None), |(key, value)| {
                    (key, Some(value.to_owned()))
                });
            if key.is_empty() || key.contains(|c: char| c.is_whitespace() || c == '\0') {
                bail!("@putenv expects an environment variable name or NAME=value.");
            }
            self.environment.insert(key.into(), value);
            return Ok(());
        }
        let binary = std::env::current_exe().context("Cannot locate the Phera executable.")?;
        let executable = shell_quote(&binary.to_string_lossy());
        let mut temporary = None;
        let command = if let Some(php_script) = script.strip_prefix("@php ") {
            if let Some(code) = php_script.strip_prefix("-r ") {
                let file = script_file(project)?;
                let path = shell_quote(&file.path().to_string_lossy());
                let args = shell_arguments(args);
                let command = format!(
                    "set -- {code}; printf '%s\\n%s\\n' '<?php' \"$1\" > {path}; \
                    shift; exec {executable} {path} \"$@\" {args}",
                );
                temporary = Some(file);
                command
            } else {
                format!("{executable} {}", forward(php_script, args))
            }
        } else if let Some(composer) = script.strip_prefix("@composer ") {
            format!("{executable} {}", forward(composer, args))
        } else if let Some(alias) = script.strip_prefix('@') {
            let (alias, parameters) = alias.split_once(char::is_whitespace).unwrap_or((alias, ""));
            if project.data["scripts"][alias].is_null() {
                bail!("Unknown Composer script alias @{alias}.");
            }
            let mut nested = split_words(parameters)?;
            nested.extend_from_slice(args);
            return self.dispatch(project, options, alias, &nested, depth + 1);
        } else if script.contains("::") && !script.contains(char::is_whitespace) {
            let Some(file) = callback(project, options, event, script, args)? else {
                eprintln!("Skipped {script}. Generate the project autoloader first.");
                return Ok(());
            };
            let command = format!(
                "{executable} {}",
                shell_quote(&file.path().to_string_lossy())
            );
            temporary = Some(file);
            command
        } else {
            forward(script, args)
        };
        if options.verbose {
            eprintln!("Running project script.");
        }
        let mut path = vec![project.bin_dir()?];
        path.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let mut child = shell(command);
        child
            .current_dir(&project.directory)
            .env("PATH", std::env::join_paths(path)?);
        for (key, value) in &self.environment {
            if let Some(value) = value {
                child.env(key, value);
            } else {
                child.env_remove(key);
            }
        }
        let status = child.status()?;
        drop(temporary);
        if !status.success() {
            bail!("Project script exited with {status}.");
        }
        Ok(())
    }

    fn flex(
        &mut self,
        project: &Project,
        options: &Options,
        scripts: &serde_json::Value,
        args: &[String],
        depth: usize,
    ) -> Result<()> {
        let packages = Lock::read(&project.directory)?.packages(options.no_dev);
        if !plugins::active(project, options, &packages, plugins::flex::NAME)? {
            eprintln!("Skipped Symfony auto-scripts. An approved symfony/flex package is needed.");
            return Ok(());
        }
        for (script, kind) in scripts.as_object().into_iter().flatten() {
            let script = plugins::flex::expand(project, script);
            let (command, args) = match kind.as_str() {
                Some("symfony-cmd") => {
                    if !packages
                        .iter()
                        .any(|package| package.name == "symfony/console")
                    {
                        eprintln!("Skipped a Symfony auto-script. Install symfony/console first.");
                        continue;
                    }
                    let path = plugins::flex::expand(project, "%BIN_DIR%/console");
                    let console = paths::inside(&project.directory, &path)?;
                    (
                        format!("@php {} {script}", shell_quote(&console.to_string_lossy())),
                        args,
                    )
                }
                Some("php-script") => (format!("@php {script}"), args),
                Some("script") => (script, &[][..]),
                _ => bail!(
                    "Unknown Symfony auto-script type. Use symfony-cmd, php-script or script."
                ),
            };
            self.run(project, options, "auto-scripts", &command, args, depth)?;
        }
        Ok(())
    }
}

fn callback(
    project: &Project,
    options: &Options,
    event: &str,
    script: &str,
    args: &[String],
) -> Result<Option<tempfile::NamedTempFile>> {
    let vendor = project.vendor_dir()?;
    let autoload = vendor.join("autoload.php");
    if !autoload.is_file() {
        return Ok(None);
    }
    let mut file = script_file(project)?;
    let optimize = options.optimize
        || options.authoritative(project)
        || project.config_bool("optimize-autoloader");
    write!(
        file,
        "<?php\n{}\nrequire {};\n\
         $event = new \\Composer\\Script\\Event({}, {}, {}, {}, ['optimize' => {optimize}]);\n\
         $callback = {};\n\
         if (!is_callable($callback)) {{ fwrite(STDERR, \"Unavailable script callback: $callback\\n\"); exit(0); }}\n\
         $result = call_user_func($callback, $event);\n\
         if (is_int($result)) {{ exit($result); }}\n",
        include_str!("../resources/script_event.php.inc"),
        php::quote(&autoload.to_string_lossy()),
        php::quote(event),
        php::quote(&vendor.to_string_lossy()),
        !options.no_dev,
        php::value(&serde_json::json!(args)),
        php::quote(script),
    )?;
    Ok(Some(file))
}

fn script_file(project: &Project) -> Result<tempfile::NamedTempFile> {
    Ok(tempfile::Builder::new()
        .prefix(".phera-script-")
        .suffix(".php")
        .tempfile_in(&project.directory)?)
}
