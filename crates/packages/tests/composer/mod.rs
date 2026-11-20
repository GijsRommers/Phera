//! Differential install, resolution, and lock handoff against Composer.

mod corpus;
mod oracle;
mod vendor;

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use corpus::Fixture;
use oracle::Php;

const INSTALL: &[&str] = &["--no-scripts", "--ignore-platform-reqs", "-o"];
const UPDATE: &[&str] = &["--no-scripts", "--ignore-platform-reqs", "--no-install"];

pub(super) fn compare() -> Result<()> {
    let work = match std::env::var_os("PHERA_PACKAGES_COMPARISON_DIR") {
        Some(path) => PathBuf::from(path),
        None => std::env::temp_dir().join("phera-package-comparison"),
    };
    fs::create_dir_all(&work)?;
    let work = work.canonicalize()?;
    let tools = Tools {
        php: Php::find(&work)?,
        phera: required_path("PHERA_BIN")?,
        composer: required_path("PHERA_COMPOSER")?,
        probe: oracle::write_probe(&work)?,
        install_only: std::env::var("PHERA_PACKAGES_INSTALL_ONLY").as_deref() == Ok("1"),
        work,
    };
    let only = std::env::var("PHERA_PACKAGES_FIXTURE").ok();
    let fixtures: Vec<_> = corpus::fixtures(&tools.work)?
        .into_iter()
        .filter(|fixture| only.as_ref().is_none_or(|name| name == &fixture.name))
        .collect();
    let pool = rayon::ThreadPoolBuilder::new().num_threads(3).build()?;
    let rows: Vec<_> = pool.install(|| {
        fixtures
            .par_iter()
            .map(|fixture| {
                let row = tools.fixture(fixture).unwrap_or_else(
                    |error| json!({"fixture": fixture.name, "error": format!("{error:#}")}),
                );
                println!("{row}");
                row
            })
            .collect()
    });
    fs::write(
        tools.work.join("results.json"),
        serde_json::to_vec_pretty(&rows)?,
    )?;
    if rows.is_empty() || rows.iter().any(|row| row["draft_checks_pass"] != true) {
        bail!("Composer comparison failed. See {}.", tools.work.display());
    }
    Ok(())
}

fn required_path(variable: &str) -> Result<PathBuf> {
    let path = std::env::var_os(variable).with_context(|| format!("Set {variable}."))?;
    Ok(PathBuf::from(path).canonicalize()?)
}

struct Tools {
    php: Php,
    phera: PathBuf,
    composer: PathBuf,
    probe: PathBuf,
    install_only: bool,
    work: PathBuf,
}

impl Tools {
    fn composer(&self, project: &Path, fixture: &Fixture) -> Command {
        let mut php = self.php.command();
        php.arg(&self.composer)
            .args(["--no-plugins", "--no-interaction"])
            .env("COMPOSER_HOME", self.work.join("composer-home"))
            .env("COMPOSER_CACHE_DIR", self.work.join("composer-cache"))
            .env("COMPOSER_ROOT_VERSION", &fixture.source.root_version)
            .current_dir(project);
        php
    }

    fn phera(&self, project: &Path, fixture: &Fixture) -> Command {
        let mut phera = command(&self.phera);
        phera
            .env("PHERA_CACHE_DIR", self.work.join("phera-store"))
            .env("COMPOSER_ROOT_VERSION", &fixture.source.root_version)
            .current_dir(project);
        phera
    }

    fn fixture(&self, fixture: &Fixture) -> Result<Value> {
        let logs = self.work.join(&fixture.name);
        fs::create_dir_all(&logs)?;
        let projects = tempfile::tempdir_in(&logs)?;
        let left = projects.path().join("composer");
        let right = projects.path().join("phera");
        fixture.prepare(&left, &logs)?;
        fixture.prepare(&right, &logs)?;
        run(
            self.composer(&left, fixture).arg("install").args(INSTALL),
            &logs,
            "composer-install",
        )?;
        run(
            self.phera(&right, fixture)
                .args(["install", "--frozen-lockfile", "--no-plugins"])
                .args(INSTALL),
            &logs,
            "phera-install",
        )?;

        let manifest: Value = serde_json::from_slice(&fixture.manifest)?;
        let vendor = manifest["config"]["vendor-dir"]
            .as_str()
            .unwrap_or("vendor");
        let left_vendor = left.join(vendor);
        let right_vendor = right.join(vendor);
        let (package_differences, generated_differences) =
            vendor::differences(&left_vendor, &right_vendor)?;
        let installed_json = |vendor: &Path| -> Result<Value> {
            Ok(serde_json::from_slice(&fs::read(
                vendor.join("composer/installed.json"),
            )?)?)
        };
        let installed_identical = installed_json(&left_vendor)? == installed_json(&right_vendor)?;
        let installed_php_identical =
            self.php
                .installed(&left, &left_vendor, &logs, "composer-installed")?
                == self
                    .php
                    .installed(&right, &right_vendor, &logs, "phera-installed")?;
        let composer_probe = oracle::observe(
            self.php
                .command()
                .args(["-d", "display_errors=stderr", "-d", "log_errors=0"]),
            &self.probe,
            &left_vendor,
            &left,
            &logs,
            "composer-probe",
        )?;
        let phera_probe = oracle::observe(
            self.php
                .command()
                .args(["-d", "display_errors=stderr", "-d", "log_errors=0"]),
            &self.probe,
            &right_vendor,
            &right,
            &logs,
            "phera-probe",
        )?;
        let runtime_probe = oracle::observe(
            &mut self.phera(&right, fixture),
            &self.probe,
            &right_vendor,
            &right,
            &logs,
            "runtime-probe",
        )?;
        let autoload_identical = composer_probe == phera_probe;
        let phera_autoload_identical = composer_probe == runtime_probe;
        let mut row = json!({
            "fixture": fixture.name,
            "package_differences": package_differences,
            "generated_differences": generated_differences,
            "installed_json_identical": installed_identical,
            "installed_php_identical": installed_php_identical,
            "autoload_identical_under_php": autoload_identical,
            "autoload_identical_under_phera": phera_autoload_identical,
            "draft_checks_pass": package_differences.is_empty()
                && installed_identical && autoload_identical,
        });
        if self.install_only {
            return Ok(row);
        }
        if let Err(error) =
            self.compare_resolution(fixture, &left, &right, &logs, &right_vendor, &mut row)
        {
            row["resolution_error"] = format!("{error:#}").into();
            row["draft_checks_pass"] = false.into();
        }
        Ok(row)
    }

    fn compare_resolution(
        &self,
        fixture: &Fixture,
        left: &Path,
        right: &Path,
        logs: &Path,
        right_vendor: &Path,
        row: &mut Value,
    ) -> Result<()> {
        fs::remove_file(left.join("composer.lock"))?;
        fs::remove_file(right.join("composer.lock"))?;
        run(
            self.composer(left, fixture)
                .args(["update", "--no-audit"])
                .args(UPDATE),
            logs,
            "composer-update",
        )?;
        run(
            self.phera(right, fixture)
                .args(["update", "--no-plugins"])
                .args(UPDATE),
            logs,
            "phera-update",
        )?;
        let composer_versions = versions(left)?;
        let phera_versions = versions(right)?;
        let differences = composer_versions
            .keys()
            .chain(phera_versions.keys())
            .filter_map(|name| {
                let expected = composer_versions.get(name);
                let actual = phera_versions.get(name);
                (expected != actual).then_some((name, (expected, actual)))
            })
            .collect::<BTreeMap<_, _>>();
        let validation = run_output(
            self.composer(right, fixture)
                .args(["validate", "--no-check-publish"]),
            logs,
            "validate-phera-lock",
        )?;
        let validation_text = format!(
            "{}{}",
            String::from_utf8_lossy(&validation.stdout),
            String::from_utf8_lossy(&validation.stderr)
        );
        let fresh_lock = !validation_text.contains("not up to date");
        fs::remove_dir_all(right_vendor)?;
        run(
            self.composer(right, fixture).arg("install").args(INSTALL),
            logs,
            "handoff",
        )?;

        row["draft_checks_pass"] =
            (row["draft_checks_pass"] == true && differences.is_empty() && fresh_lock).into();
        row["resolution_differences"] = json!(differences);
        row["composer_accepts_lock"] = fresh_lock.into();
        row["composer_installs_phera_lock"] = true.into();
        Ok(())
    }
}

fn versions(project: &Path) -> Result<BTreeMap<String, String>> {
    let lock = phera_packages::Lock::read(project)?;
    Ok(lock
        .packages
        .into_iter()
        .chain(lock.packages_dev)
        .map(|package| (package.name, package.version))
        .collect())
}

fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.env_clear().stdin(Stdio::null());
    for name in ["PATH", "HOME", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

fn run_output(command: &mut Command, logs: &Path, name: &str) -> Result<Output> {
    let output = command.output().with_context(|| format!("Run {name}"))?;
    let path = logs.join(format!("{name}.log"));
    let mut log = output.stdout.clone();
    log.extend_from_slice(&output.stderr);
    fs::write(path, log)?;
    Ok(output)
}

fn run(command: &mut Command, logs: &Path, name: &str) -> Result<String> {
    let output = run_output(command, logs, name)?;
    if !output.status.success() {
        bail!(
            "{name} exited with {}. See {}/{name}.log.",
            output.status,
            logs.display()
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}
