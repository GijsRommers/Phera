//! Write Composer locks and divide production dependencies from development packages.
//!
//! A package reachable from root `require` belongs in `packages`, even if dev also needs it.

use crate::{Options, Package, Project, content_hash, version::Stability};
use anyhow::Result;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

pub(crate) fn build(project: &Project, options: &Options, packages: &[Package]) -> Result<Value> {
    let mut production = BTreeSet::new();
    let mut pending = keys(&project.data["require"]);
    while let Some(name) = pending.pop() {
        if !production.insert(name.clone()) {
            continue;
        }
        if let Some(package) = packages.iter().find(|package| {
            package.name == name
                || ["provide", "replace"]
                    .iter()
                    .any(|key| package.field(key).get(&name).is_some())
        }) {
            production.insert(package.name.clone());
            pending.extend(keys(package.field("require")));
        }
    }
    let dump = |package: &Package| -> Result<Value> {
        let mut value = serde_json::to_value(package)?;
        if let Some(map) = value.as_object_mut() {
            for field in [
                "version_normalized",
                "uid",
                "notification-url",
                "installation-source",
                "install-path",
                "default-branch",
            ] {
                map.remove(field);
            }
        }
        Ok(value)
    };
    let regular: Vec<Value> = packages
        .iter()
        .filter(|package| production.contains(&package.name))
        .map(dump)
        .collect::<Result<_>>()?;
    let development: Vec<Value> = packages
        .iter()
        .filter(|package| !production.contains(&package.name))
        .map(dump)
        .collect::<Result<_>>()?;
    let mut flags = Map::new();
    let mut aliases = Vec::new();
    for key in ["require", "require-dev"] {
        if let Some(requirements) = project.data[key].as_object() {
            for (name, constraint) in requirements {
                let Some(text) = constraint.as_str() else {
                    continue;
                };
                let stability = text
                    .rsplit_once('@')
                    .and_then(|(_, flag)| Stability::parse(flag))
                    .or_else(|| text.contains("dev").then_some(Stability::Dev));
                if let Some(stability) = stability {
                    let flag = match stability {
                        Stability::Stable => 0,
                        Stability::Rc => 5,
                        Stability::Beta => 10,
                        Stability::Alpha => 15,
                        Stability::Dev => 20,
                    };
                    flags.insert(name.clone(), flag.into());
                }
                if let Some((source, alias)) = text.split_once(" as ") {
                    aliases.push(json!({"package": name,
                        "version": crate::version::normalize(source)?, "alias": alias,
                        "alias_normalized": crate::version::normalize(alias)?}));
                }
            }
        }
    }
    let flags = if flags.is_empty() {
        json!([])
    } else {
        Value::Object(flags)
    };
    let mut value = json!({
        "_readme": ["This file locks the dependencies of your project to a known state",
            concat!("Read more about it at https://getcomposer.org/doc/",
                "01-basic-usage.md#installing-dependencies"),
            "This file is @generated automatically"],
        "content-hash": content_hash::compute(&project.data)?,
        "packages": regular, "packages-dev": development, "aliases": aliases,
        "minimum-stability": project.data["minimum-stability"].as_str().unwrap_or("stable"),
        "stability-flags": flags, "prefer-stable": project.data["prefer-stable"] == true,
        "prefer-lowest": options.prefer_lowest,
        "platform": platform(&project.data["require"]),
        "platform-dev": platform(&project.data["require-dev"]), "plugin-api-version": "2.6.0"
    });
    if let Some(platform) = project.data["config"].get("platform") {
        value["platform-overrides"] = platform.clone();
    }
    Ok(value)
}

fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|items| items.keys().cloned().collect())
        .unwrap_or_default()
}

fn platform(requirements: &Value) -> Value {
    let mut platform = Map::new();
    if let Some(requirements) = requirements.as_object() {
        for (name, constraint) in requirements {
            if crate::solver::is_platform(name) {
                platform.insert(name.clone(), constraint.clone());
            }
        }
    }
    if platform.is_empty() {
        json!([])
    } else {
        Value::Object(platform)
    }
}
