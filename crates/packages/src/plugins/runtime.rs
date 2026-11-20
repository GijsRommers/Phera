//! Generate Symfony's `vendor/autoload_runtime.php` from its installed template.
//!
//! A Symfony front controller can then `require_once '../vendor/autoload_runtime.php'`.

use crate::{Project, paths, php};
use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::Path;

pub(super) const NAME: &str = "symfony/runtime";

pub(super) fn write(project: &Project) -> Result<()> {
    let extra = &project.data["extra"]["runtime"];
    if extra == false {
        return Ok(());
    }
    let vendor = project.vendor_dir()?;
    let destination = paths::inside(&vendor, "autoload_runtime.php")?;
    let template = match extra["autoload_template"].as_str() {
        Some(path) => paths::inside(&project.directory, path)?,
        None => paths::inside(
            &vendor,
            "symfony/runtime/Internal/autoload_runtime.template",
        )?,
    };
    let template = fs::read_to_string(template).context("Read the Symfony Runtime template.")?;
    let directory = paths::inside(
        &project.directory,
        extra["project_dir"].as_str().unwrap_or(""),
    )?;
    let directory = directory_expression(&vendor, &directory);
    let class = extra["class"]
        .as_str()
        .unwrap_or("Symfony\\Component\\Runtime\\SymfonyRuntime");
    let mut options = String::from("[\n");
    if let Some(extra) = extra.as_object() {
        for (key, value) in extra {
            if !matches!(key.as_str(), "class" | "autoload_template" | "project_dir") {
                writeln!(options, "  {} => {},", php::quote(key), php::value(value))?;
            }
        }
    }
    writeln!(options, "  'project_dir' => {directory},")?;
    options.push(']');
    let code = template
        .replace("%project_dir%", &directory)
        .replace("%runtime_class%", &php::quote(class))
        .replace("%runtime_options%", &options);
    if fs::read(&destination).is_ok_and(|previous| previous == code.as_bytes()) {
        return Ok(());
    }
    let mut file = tempfile::NamedTempFile::new_in(vendor)?;
    file.write_all(code.as_bytes())?;
    file.persist(destination)?;
    Ok(())
}

fn directory_expression(vendor: &Path, project: &Path) -> String {
    let relative = php::relative(vendor, project);
    let mut parts = relative.split('/').peekable();
    let mut parents = 0;
    while parts.next_if_eq(&"..").is_some() {
        parents += 1;
    }
    let remainder = parts.collect::<Vec<_>>().join("/");
    let base = if parents == 0 {
        "__DIR__".into()
    } else {
        format!("dirname(__DIR__, {parents})")
    };
    if remainder.is_empty() {
        base
    } else {
        format!("{base}.{}", php::quote(&format!("/{remainder}/")))
    }
}
