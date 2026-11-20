//! Composer's installed package records and runtime version API.
//!
//! `Composer\InstalledVersions::getPrettyVersion('psr/log')` works under plain PHP.

use crate::{InstallPaths, Lock, Options, Package, Project, php};
use anyhow::Result;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::{fs, path::Path};

pub(crate) mod read;

pub(crate) fn write(
    project: &Project,
    lock: &Lock,
    options: &Options,
    paths: &InstallPaths,
) -> Result<()> {
    project.validate_output_paths()?;
    let vendor = project.vendor_dir()?;
    let directory = vendor.join("composer");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join("InstalledVersions.php"),
        include_bytes!("../resources/InstalledVersions.php"),
    )?;
    let packages = lock.packages(options.no_dev);
    let dev_names: Vec<_> = lock
        .packages_dev
        .iter()
        .filter(|_| !options.no_dev)
        .map(|package| package.name.as_str())
        .collect();
    let installed: Vec<Value> = packages
        .iter()
        .map(|package| -> Result<Value> {
            let mut value = serde_json::to_value(package)?;
            value["version_normalized"] = normalized(package).into();
            if package.kind() != "metapackage" {
                value["installation-source"] = if package.field("dist").is_null() {
                    "source".into()
                } else {
                    "dist".into()
                };
            }
            value["install-path"] = if package.kind() == "metapackage" {
                Value::Null
            } else {
                relative_install_path(&directory, &paths.installed(package)?).into()
            };
            Ok(ordered_metadata(value))
        })
        .collect::<Result<_>>()?;
    let record = json!({"packages": installed, "dev": !options.no_dev,
        "dev-package-names": dev_names});
    fs::write(directory.join("installed.json"), metadata_json(&record)?)?;
    let root_version = project.root_version();
    let normalized_root = crate::version::normalize(&root_version)?;
    let root_relative = php::relative(&directory, &project.directory);
    let root_path = format!("__DIR__ . {}", php::quote(&format!("/{root_relative}/")));
    let root_type = project.data["type"].as_str().unwrap_or("library");
    let root = json!({
        "name": project.name(), "pretty_version": root_version,
        "version": normalized_root, "reference": null, "type": root_type,
        "aliases": [], "dev": !options.no_dev
    });
    let mut output = String::from("<?php return array(\n    'root' => ");
    entry(&mut output, &root, Some(&root_path), "        ");
    output.push_str(",\n    'versions' => array(\n");
    let mut versions = BTreeMap::from([(
        project.name().to_owned(),
        json!({
            "pretty_version": root_version, "version": normalized_root, "reference": null,
            "type": root_type, "aliases": [], "dev_requirement": false,
        }),
    )]);
    for package in &packages {
        let dev = dev_names.contains(&package.name.as_str());
        let mut aliases = Vec::new();
        if let Some(alias) = package.field("extra")["branch-alias"]
            .get(&package.version)
            .and_then(Value::as_str)
            && let Ok(version) = crate::version::Version::branch_alias(alias)
        {
            let normalized = version.normalized();
            aliases.push(match normalized.split_once(".9999999") {
                Some((prefix, _)) => format!("{prefix}.x-dev"),
                None => normalized.to_owned(),
            });
        }
        if let Some(locked_aliases) = lock.other.get("aliases").and_then(Value::as_array) {
            for alias in locked_aliases
                .iter()
                .filter(|alias| alias["package"] == package.name)
            {
                if let Some(alias) = alias["alias"].as_str() {
                    aliases.push(alias.to_owned());
                }
            }
        }
        aliases.sort();
        aliases.dedup();
        versions.insert(
            package.name.clone(),
            json!({
                "pretty_version": package.version, "version": normalized(package),
                "reference": package.field("dist")["reference"].as_str()
                    .or(package.field("source")["reference"].as_str()),
                "type": package.kind(), "aliases": aliases, "dev_requirement": dev
            }),
        );
    }
    let root_package = Package {
        name: project.name().into(),
        version: root_version,
        data: project.data.as_object().cloned().unwrap_or_default(),
    };
    for package in packages.iter().chain(std::iter::once(&root_package)) {
        for (field, key) in [("replace", "replaced"), ("provide", "provided")] {
            if let Some(links) = package.field(field).as_object() {
                for (name, constraint) in links {
                    if crate::solver::is_platform(name) {
                        continue;
                    }
                    let constraint = if constraint == "self.version" {
                        Value::String(package.version.clone())
                    } else {
                        constraint.clone()
                    };
                    let entry = versions.entry(name.clone()).or_insert_with(|| {
                        json!({
                            "dev_requirement": dev_names.contains(&package.name.as_str())
                        })
                    });
                    if entry.get(key).is_none() {
                        entry[key] = json!([]);
                    }
                    if !dev_names.contains(&package.name.as_str()) {
                        entry["dev_requirement"] = false.into();
                    }
                    if let Some(items) = entry[key].as_array_mut()
                        && !items.contains(&constraint)
                    {
                        items.push(constraint);
                    }
                }
            }
        }
    }
    for (name, mut value) in versions {
        for key in ["provided", "replaced"] {
            if let Some(links) = value.get_mut(key).and_then(Value::as_array_mut) {
                links.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
            }
        }
        let path = if name == project.name() {
            Some(root_path.clone())
        } else if let Some(package) = packages.iter().find(|package| package.name == name) {
            Some(if package.kind() == "metapackage" {
                "null".into()
            } else {
                let relative = relative_install_path(&directory, &paths.installed(package)?);
                format!("__DIR__ . {}", php::quote(&format!("/{relative}")))
            })
        } else {
            None
        };
        write!(output, "        {} => ", php::quote(&name))?;
        entry(&mut output, &value, path.as_deref(), "            ");
        output.push_str(",\n");
    }
    output.push_str("    ),\n);\n");
    fs::write(directory.join("installed.php"), output)?;
    Ok(())
}

fn ordered_metadata(value: Value) -> Value {
    const FIELDS: &[&str] = &[
        "name",
        "version",
        "version_normalized",
        "target-dir",
        "source",
        "dist",
        "require",
        "conflict",
        "provide",
        "replace",
        "require-dev",
        "suggest",
        "time",
        "published-time",
        "default-branch",
        "bin",
        "type",
        "extra",
        "installation-source",
        "autoload",
        "autoload-dev",
        "notification-url",
        "include-path",
        "php-ext",
        "archive",
        "scripts",
        "license",
        "authors",
        "description",
        "homepage",
        "keywords",
        "repositories",
        "support",
        "funding",
        "abandoned",
        "transport-options",
        "install-path",
    ];
    let Value::Object(mut fields) = value else {
        return value;
    };
    for field in [
        "require",
        "conflict",
        "provide",
        "replace",
        "require-dev",
        "suggest",
    ] {
        if let Some(values) = fields.get_mut(field).and_then(Value::as_object_mut) {
            values.sort_keys();
        }
    }
    if let Some(keywords) = fields.get_mut("keywords").and_then(Value::as_array_mut) {
        keywords.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
    }
    let mut ordered = serde_json::Map::new();
    for field in FIELDS {
        if let Some(value) = fields.shift_remove(*field) {
            ordered.insert((*field).into(), value);
        }
    }
    ordered.extend(fields);
    Value::Object(ordered)
}

fn metadata_json(record: &Value) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    record.serialize(&mut serde_json::Serializer::with_formatter(
        &mut output,
        formatter,
    ))?;
    output.push(b'\n');
    Ok(output)
}

fn entry(output: &mut String, value: &Value, install_path: Option<&str>, indent: &str) {
    output.push_str("array(\n");
    for (key, value) in value.as_object().into_iter().flatten() {
        output.push_str(&format!("{indent}{} => ", php::quote(key)));
        match value.as_array() {
            Some(items) if !items.is_empty() => {
                output.push_str("array(\n");
                for (item, value) in items.iter().enumerate() {
                    output.push_str(&format!("{indent}    {item} => {},\n", php::value(value)));
                }
                output.push_str(&format!("{indent}),\n"));
            }
            _ => output.push_str(&format!("{},\n", php::value(value))),
        }
        if key == "type"
            && let Some(path) = install_path
        {
            output.push_str(&format!("{indent}'install_path' => {path},\n"));
        }
    }
    output.push_str(&indent[4..]);
    output.push(')');
}

fn relative_install_path(directory: &Path, path: &Path) -> String {
    let relative = php::relative(directory, path);
    if relative.starts_with('.') {
        relative
    } else {
        format!("./{relative}")
    }
}

fn normalized(package: &Package) -> String {
    package
        .field("version_normalized")
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| {
            crate::version::normalize(&package.version).unwrap_or_else(|_| package.version.clone())
        })
}
