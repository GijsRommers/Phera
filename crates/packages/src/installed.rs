//! Composer's installed package records and runtime version API.
//!
//! `Composer\InstalledVersions::getPrettyVersion('psr/log')` works under plain PHP.

use crate::{Lock, Options, Package, Project, php};
use anyhow::Result;
use serde_json::{Value, json};
use std::fs;

pub(crate) fn write(project: &Project, lock: &Lock, options: &Options) -> Result<()> {
    let vendor = project.vendor_dir()?;
    let directory = vendor.join("composer");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join("InstalledVersions.php"),
        include_bytes!("../resources/InstalledVersions.php"),
    )?;
    fs::write(
        directory.join("LICENSE"),
        include_bytes!("../resources/LICENSE.composer"),
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
            value["installation-source"] = if package.field("dist").is_null() {
                "source".into()
            } else {
                "dist".into()
            };
            value["install-path"] = if package.kind() == "metapackage" {
                Value::Null
            } else {
                {
                    let relative = php::relative(&directory, &vendor.join(&package.name));
                    if relative.starts_with('.') {
                        relative
                    } else {
                        format!("./{relative}")
                    }
                }
                .into()
            };
            Ok(value)
        })
        .collect::<Result<_>>()?;
    let record = json!({"packages": installed, "dev": !options.no_dev,
        "dev-package-names": dev_names});
    fs::write(
        directory.join("installed.json"),
        serde_json::to_string_pretty(&record)? + "\n",
    )?;
    let root_version = project.root_version();
    let root = json!({
        "name": project.name(), "pretty_version": root_version,
        "version": "1.0.0.0", "reference": null, "type": "project",
        "aliases": [], "dev": true
    });
    let mut output = String::from("<?php return array(\n    'root' => ");
    let root_code = php::value(&root);
    output.push_str(&root_code.replacen("array(", "array('install_path' => $baseDir, ", 1));
    output.push_str(",\n    'versions' => array(\n");
    output.push_str(&format!(
        "        {} => array('pretty_version' => {}, \
        'version' => '1.0.0.0', 'aliases' => array(), 'reference' => null, \
        'type' => 'project', 'install_path' => $baseDir, 'dev_requirement' => false),\n",
        php::quote(project.name()),
        php::quote(&root_version)
    ));
    let mut versions = serde_json::Map::<String, Value>::new();
    for package in &packages {
        let dev = dev_names.contains(&package.name.as_str());
        versions.insert(
            package.name.clone(),
            json!({
                "pretty_version": package.version, "version": normalized(package),
                "reference": package.field("dist")["reference"].as_str()
                    .or(package.field("source")["reference"].as_str()),
                "type": package.kind(), "aliases": [], "dev_requirement": dev
            }),
        );
    }
    for package in &packages {
        for (field, key) in [("replace", "replaced"), ("provide", "provided")] {
            if let Some(links) = package.field(field).as_object() {
                for (name, constraint) in links {
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
                    if let Some(items) = entry[key].as_array_mut() {
                        items.push(constraint);
                    }
                }
            }
        }
    }
    let mut entries: Vec<_> = versions.iter().collect();
    entries.sort_by_key(|(name, _)| *name);
    for (name, value) in entries {
        let path = if value.get("version").is_some()
            && packages
                .iter()
                .any(|package| package.name == *name && package.kind() != "metapackage")
        {
            php::path(&vendor.join(name), &vendor, &project.directory)
        } else {
            "null".into()
        };
        let code =
            php::value(value).replacen("array(", &format!("array('install_path' => {path}, "), 1);
        output.push_str(&format!("        {} => {code},\n", php::quote(name)));
    }
    output.push_str("    ),\n);\n");
    output = output
        .replace(
            "$baseDir",
            &format!(
                "__DIR__ . {}",
                php::quote(&format!(
                    "/{}",
                    php::relative(&directory, &project.directory)
                ))
            ),
        )
        .replace("$vendorDir", "__DIR__ . '/..'");
    fs::write(directory.join("installed.php"), output)?;
    Ok(())
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
