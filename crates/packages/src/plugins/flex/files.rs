//! Build Symfony's recipe files, bundle registrations and dotenv defaults.

use crate::{Project, archive, paths, php};
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub(super) struct Outputs<'a> {
    project: &'a Project,
    vendor: PathBuf,
    files: BTreeMap<PathBuf, (Vec<u8>, bool)>,
    bundles: BTreeMap<String, Vec<String>>,
    env: BTreeMap<String, Vec<String>>,
}

impl<'a> Outputs<'a> {
    pub(super) fn new(project: &'a Project) -> Result<Self> {
        Ok(Self {
            project,
            vendor: project.vendor_dir()?,
            files: BTreeMap::new(),
            bundles: BTreeMap::new(),
            env: BTreeMap::new(),
        })
    }

    pub(super) fn present(&self, entry: &Value) -> Result<bool> {
        if !self.bundles_path()?.is_file() || !self.destination(".env")?.is_file() {
            return Ok(false);
        }
        for file in crate::project::strings(&entry["files"]) {
            if !self.destination(file)?.is_file() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn add(&mut self, name: &str, recipe: &Value) -> Result<Vec<String>> {
        let manifest = &recipe["manifest"];
        let mut paths = Vec::new();
        for (source, file) in fields(&recipe["files"]) {
            for (from, to) in fields(&manifest["copy-from-recipe"]) {
                let Some(to) = to.as_str() else { continue };
                let tail = if source == from {
                    ""
                } else if let Some(tail) = source.strip_prefix(from).filter(|_| from.ends_with('/'))
                {
                    tail
                } else {
                    continue;
                };
                let path = self.expand(&format!("{to}{tail}"));
                let destination = self.destination(&path)?;
                let contents = if file["contents"].is_array() {
                    let lines = crate::project::strings(&file["contents"]);
                    self.expand(&lines.join("\n")).into_bytes()
                } else {
                    let bytes = file["contents"]
                        .as_str()
                        .context("Invalid recipe file contents.")?;
                    STANDARD
                        .decode(bytes)
                        .context("Invalid base64 in Symfony recipe.")?
                };
                let executable = file["executable"].as_bool().unwrap_or(false);
                self.files
                    .entry(destination)
                    .or_insert((contents, executable));
                paths.push(path);
            }
        }
        for (class, modes) in fields(&manifest["bundles"]) {
            self.bundles.insert(
                class.clone(),
                crate::project::strings(modes)
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            );
        }
        let env = fields(&manifest["env"])
            .filter_map(|(key, value)| Some((key.as_str(), value.as_str()?)))
            .map(|(key, value)| {
                if key.starts_with('#') {
                    format!("# {value}")
                } else {
                    format!("{key}={}", self.expand(value))
                }
            })
            .collect::<Vec<_>>();
        if !env.is_empty() {
            self.env.insert(name.to_owned(), env);
        }
        Ok(paths)
    }

    pub(super) fn write(mut self) -> Result<()> {
        if !self.bundles.is_empty() {
            let mut php = String::from("<?php\n\nreturn [\n");
            for (class, modes) in &self.bundles {
                if class.is_empty()
                    || !class
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '\\'))
                {
                    bail!("Invalid class in Symfony recipe bundle registration.");
                }
                let modes = modes
                    .iter()
                    .map(|mode| format!("{} => true", php::quote(mode)))
                    .collect::<Vec<_>>();
                php.push_str(&format!("    {class}::class => [{}],\n", modes.join(", ")));
            }
            php.push_str("];\n");
            self.files
                .insert(self.bundles_path()?, (php.into_bytes(), false));
        }
        if !self.env.is_empty() {
            let env_path = self.destination(".env")?;
            let (contents, _) = self.files.entry(env_path).or_default();
            for (name, lines) in &self.env {
                writeln!(contents, "\n###> {name} ###")?;
                writeln!(contents, "{}", lines.join("\n"))?;
                writeln!(contents, "###< {name} ###")?;
            }
        }
        for (path, (contents, executable)) in self.files {
            create(&path, &contents, executable)?;
        }
        Ok(())
    }

    fn expand(&self, input: &str) -> String {
        super::expand(self.project, input)
    }

    fn bundles_path(&self) -> Result<PathBuf> {
        self.destination(&self.expand("%CONFIG_DIR%/bundles.php"))
    }

    fn destination(&self, source: &str) -> Result<PathBuf> {
        let path = paths::inside(&self.project.directory, source)?;
        let relative = path.strip_prefix(&self.project.directory)?;
        let protected = relative.components().any(|component| {
            matches!(component, Component::Normal(part) if part.eq_ignore_ascii_case(".git"))
        }) || relative.components().next().is_some_and(|component| {
            ["composer.json", "composer.lock", "auth.json", "symfony.lock"]
                .iter()
                .any(|name| component.as_os_str().eq_ignore_ascii_case(name))
        });
        if relative.as_os_str().is_empty() || path.starts_with(&self.vendor) || protected {
            bail!("Protected Symfony recipe destination: {}.", path.display());
        }
        Ok(path)
    }
}

fn fields(value: &Value) -> impl Iterator<Item = (&String, &Value)> {
    value.as_object().into_iter().flatten()
}

fn create(path: &Path, contents: &[u8], executable: bool) -> Result<()> {
    if path.symlink_metadata().is_ok() {
        return Ok(());
    }
    let parent = path
        .parent()
        .context("Symfony recipe file needs a directory.")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(contents)?;
    archive::set_mode(file.path(), if executable { 0o755 } else { 0o644 })?;
    file.persist_noclobber(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn creates_recipe_data_and_preserves_existing_files() {
        let directory = tempfile::tempdir().unwrap();
        let project = Project {
            directory: directory.path().canonicalize().unwrap(),
            data: json!({"extra": {"config-dir": "settings"}}),
        };
        let mut outputs = Outputs::new(&project).unwrap();
        let recipe = json!({
            "manifest": {
                "copy-from-recipe": {"config/": "%CONFIG_DIR%/"},
                "bundles": {"Symfony\\Example": ["all"]},
                "env": {"APP_ENV": "dev"},
            },
            "files": {
                "config/defaults.yaml": {"contents": ["directory: %CONFIG_DIR%", ""]},
                "config/binary.bin": {"contents": "AAE="},
            },
        });
        outputs.add("symfony/example", &recipe).unwrap();
        assert!(outputs.destination(".git/config").is_err());
        assert!(outputs.destination("composer.json").is_err());
        assert!(outputs.destination("vendor/autoload.php").is_err());
        fs::create_dir(project.directory.join("settings")).unwrap();
        fs::write(project.directory.join("settings/defaults.yaml"), "existing").unwrap();
        outputs.write().unwrap();
        assert_eq!(
            fs::read(project.directory.join("settings/defaults.yaml")).unwrap(),
            b"existing"
        );
        assert_eq!(
            fs::read(project.directory.join("settings/binary.bin")).unwrap(),
            [0, 1]
        );
        assert!(
            fs::read_to_string(project.directory.join("settings/bundles.php"))
                .unwrap()
                .contains("Symfony\\Example::class => ['all' => true]")
        );
        assert!(
            fs::read_to_string(project.directory.join(".env"))
                .unwrap()
                .contains("APP_ENV=dev")
        );
    }
}
