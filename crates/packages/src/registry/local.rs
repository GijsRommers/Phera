//! Read local path and artifact repositories without executing package code.
//!
//! `{ "type": "path", "url": "packages/*" }` loads each package's composer.json.

use crate::{Package, Project, store::Store};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::fs;

pub(super) fn packages(
    project: &Project,
    repository: &Value,
    store: &Store,
) -> Result<Vec<Package>> {
    let url = repository["url"]
        .as_str()
        .context("Local repository needs a url directory.")?;
    let pattern = crate::paths::inside(&project.directory, url)?;
    let mut packages = Vec::new();
    for item in glob::glob(&pattern.to_string_lossy())? {
        let path = item?.canonicalize()?;
        if !path.starts_with(&project.directory) {
            bail!("Local repository must stay inside the project directory.");
        }
        if repository["type"] == "path" {
            let manifest: Value = serde_json::from_slice(&fs::read(path.join("composer.json"))?)?;
            let name = manifest["name"]
                .as_str()
                .context("Path package needs a name.")?;
            let version = repository["options"]["versions"][name]
                .as_str()
                .or(manifest["version"].as_str())
                .unwrap_or("dev-main");
            let mut data = manifest
                .as_object()
                .context("Path manifest must be an object.")?
                .clone();
            data.insert("version".into(), version.into());
            let mut transport = serde_json::Map::new();
            for key in ["symlink", "relative"] {
                if let Some(value) = repository["options"][key].as_bool() {
                    transport.insert(key.into(), value.into());
                }
            }
            if !transport.is_empty() {
                data.insert("transport-options".into(), Value::Object(transport));
            }
            data.insert(
                "dist".into(),
                json!({"type": "path",
                "url": path.strip_prefix(&project.directory)?.to_string_lossy(),
                "reference": format!("{:x}", sha1::Sha1::digest(serde_json::to_vec(&manifest)?))}),
            );
            packages.push(serde_json::from_value(Value::Object(data))?);
        } else {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let file = entry.path();
                let Some(extension) = file.extension().and_then(|value| value.to_str()) else {
                    continue;
                };
                let kind = match extension {
                    "zip" => "zip",
                    "gz" | "tgz" => "tar.gz",
                    "tar" => "tar",
                    _ => continue,
                };
                let temporary = tempfile::tempdir_in(store.directory.join("staging"))?;
                crate::archive::extract(&fs::read(&file)?, kind, temporary.path())?;
                let manifest = walkdir::WalkDir::new(temporary.path())
                    .max_depth(2)
                    .into_iter()
                    .filter_map(std::result::Result::ok)
                    .find(|entry| entry.file_name() == "composer.json")
                    .context("Artifact archive must contain composer.json.")?;
                let mut data: Value = serde_json::from_slice(&fs::read(manifest.path())?)?;
                data["dist"] = json!({"type": kind, "url": format!("file://{}", file.display()),
                    "shasum": format!("{:x}", sha1::Sha1::digest(fs::read(file)?))});
                packages.push(serde_json::from_value(data)?);
            }
        }
    }
    Ok(packages)
}

use sha1::Digest;
