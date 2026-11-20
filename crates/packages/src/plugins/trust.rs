//! Read or record the root project's Composer `allow-plugins` decision.

use crate::{Lock, Options, Package, Project, update, version::Version};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::io::{self, IsTerminal, Write};

pub(crate) fn prepare(
    project: &mut Project,
    lock: &Lock,
    options: &Options,
    packages: &[Package],
) -> Result<()> {
    if options.no_plugins {
        return Ok(());
    }
    let interactive = !options.no_interaction
        && std::env::var_os("COMPOSER_NO_INTERACTION").is_none()
        && io::stdin().is_terminal();
    let original = project.data["config"]["allow-plugins"].clone();
    if !project.data["config"].is_null() && !project.data["config"].is_object() {
        bail!("config in composer.json must be an object.");
    }
    if !original.is_null()
        && !original.is_boolean()
        && !original.is_object()
        && !original.as_array().is_some_and(Vec::is_empty)
    {
        bail!("config.allow-plugins must be a boolean or a package-to-boolean map.");
    }
    let mut persistent = original.as_object().cloned().unwrap_or_default();
    let previous_api = lock.other.get("plugin-api-version").and_then(Value::as_str);
    let legacy = persistent.is_empty()
        && !original.is_boolean()
        && previous_api
            .and_then(|api| Version::parse(api).ok())
            .zip(Version::parse("2.2.0").ok())
            .is_some_and(|(old, current)| old < current);
    let mut recorded = false;
    for package in packages
        .iter()
        .filter(|package| matches!(package.kind(), "composer-plugin" | "composer-installer"))
    {
        let name = &package.name;
        if super::allowed(project, name)?.is_some()
            || name == "composer/package-versions-deprecated"
        {
            continue;
        }
        if legacy && !interactive {
            bail!(
                "This lock predates Composer allow-plugins. Refresh the lock with an explicit allow-plugins configuration."
            );
        }
        let optional = package.field("extra")["plugin-optional"] == true;
        let answer = if interactive {
            ask(name)?
        } else if optional {
            Answer::Discard
        } else {
            bail!(
                "{name} is blocked by config.allow-plugins. Set it to true to enable it, \
                 or false to explicitly skip it."
            );
        };
        if let Answer::Remember(allow) = answer {
            recorded = true;
            persistent.insert(name.clone(), allow.into());
        }
        if !project.data["config"]["allow-plugins"].is_object() {
            project.data["config"]["allow-plugins"] = json!({});
        }
        project.data["config"]["allow-plugins"][name] =
            matches!(answer, Answer::Remember(true)).into();
    }
    if recorded {
        let mut manifest = project.data.clone();
        if project.config_bool("sort-packages") {
            persistent.sort_keys();
        }
        manifest["config"]["allow-plugins"] = Value::Object(persistent);
        update::write_json(&project.directory.join("composer.json"), &manifest)?;
    }
    Ok(())
}

enum Answer {
    Remember(bool),
    Discard,
}

fn ask(name: &str) -> Result<Answer> {
    for _ in 0..6 {
        eprint!("Trust Composer plugin {name}? [y,n,d,?] ");
        io::stderr().flush()?;
        let mut answer = String::new();
        if io::stdin().read_line(&mut answer)? == 0 {
            break;
        }
        match answer.trim() {
            "y" => return Ok(Answer::Remember(true)),
            "n" => return Ok(Answer::Remember(false)),
            "d" => return Ok(Answer::Discard),
            _ => eprintln!("y: allow and save, n: deny and save, d: skip without saving."),
        }
    }
    bail!("{name} is blocked until you decide whether to allow it.")
}
