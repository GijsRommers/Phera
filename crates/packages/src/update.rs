//! Solve updates and publish matching composer.json and composer.lock files.

use crate::{Lock, Options, Package, Project, content_hash, lock_writer, platform, plugins};
use crate::{
    install_locked,
    registry::Registry,
    scripts::Runner,
    solver,
    version::{Stability, Version},
};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;

pub(crate) struct Edit {
    pub(crate) inferred: Vec<(String, String)>,
    pub(crate) original: Vec<u8>,
}

pub(crate) fn run(
    mut project: Project,
    names: &[String],
    edit: Option<Edit>,
    options: &Options,
) -> Result<()> {
    let path = project.directory.join("composer.lock");
    let previous = match fs::read(&path) {
        Ok(bytes) => {
            Some(serde_json::from_slice::<Value>(&bytes).context("Invalid composer.lock.")?)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if options.refresh_lock {
        if !names.is_empty() || edit.is_some() || options.with_all_dependencies {
            bail!("Use --lock by itself to refresh the content hash.");
        }
        return refresh_hash(&project, &path, previous, options.dry_run);
    }
    let previous = previous.map(serde_json::from_value::<Lock>).transpose()?;
    if edit.is_none() && !names.is_empty() {
        solver::locked::validate_request(names, previous.as_ref())?;
    }
    let mut runner = Runner::default();
    if !options.dry_run && edit.is_none() {
        runner.event(&project, options, "pre-update-cmd")?;
    }
    let registry = Registry::new(&project, options)?;
    let transitive = options
        .with_all_dependencies
        .then_some((&project, &registry));
    let pinned = solver::locked::Locked::select(previous.as_ref(), names, transitive)?;
    let packages = solver::solve(&project, options, &registry, &pinned)?;
    if let Some(edit) = &edit {
        for (section, name) in &edit.inferred {
            let found = packages
                .iter()
                .find(|package| package.name == *name)
                .with_context(|| format!("Pass a version constraint for {name}."))?;
            project.data[section][name] = recommended(found)?.into();
        }
    }
    let lock = lock_writer::build(&project, options, &packages)?;
    let next = serde_json::from_value::<Lock>(lock.clone())?;
    if !options.no_install {
        platform::validate(&project, &next, options)?;
        plugins::validate(&project, options, &next.packages(options.no_dev))?;
        project.validate_output_paths()?;
    }
    print_changes(previous.as_ref(), &packages);
    if options.dry_run {
        return Ok(());
    }
    let manifest = project.directory.join("composer.json");
    if let Some(edit) = &edit {
        write_json(&manifest, &project.data)?;
        let result = runner
            .event(&project, options, "pre-update-cmd")
            .and_then(|()| write_json(&path, &lock));
        if let Err(error) = result {
            write_file(&manifest, &edit.original)
                .with_context(|| format!("Could not restore composer.json after {error:#}"))?;
            return Err(error.context("Restored composer.json after failed update."));
        }
    } else {
        write_json(&path, &lock)?;
    }
    if options.no_install {
        crate::audit::summary(&project, options, &next.packages(options.no_dev));
        return Ok(());
    }
    install_locked(&project.directory, options, true, &mut runner)
}

fn recommended(package: &Package) -> Result<String> {
    let version = Version::parse(&package.version)?;
    let stability = version.stability();
    let normalized = if stability == Stability::Dev {
        let Some(alias) = package.field("extra")["branch-alias"][&package.version]
            .as_str()
            .and_then(|alias| Version::branch_alias(alias).ok())
        else {
            return Ok(package.version.clone());
        };
        alias.normalized().replace("9999999", "0")
    } else {
        version.normalized().into()
    };
    let numeric = normalized.split('-').next().unwrap_or_default();
    let parts: Vec<_> = numeric.split('.').collect();
    if parts.len() != 4 {
        return Ok(package.version.clone());
    }
    let count = if parts[0] == "0" { 3 } else { 2 };
    let flag = match stability {
        Stability::Stable => "",
        Stability::Rc => "@RC",
        Stability::Beta => "@beta",
        Stability::Alpha => "@alpha",
        Stability::Dev => "@dev",
    };
    Ok(format!("^{}{flag}", parts[..count].join(".")))
}

fn refresh_hash(project: &Project, path: &Path, previous: Option<Value>, dry: bool) -> Result<()> {
    let mut previous = previous.context("No composer.lock found. Run `phera update` first.")?;
    let _ = serde_json::from_value::<Lock>(previous.clone())?;
    let hash = content_hash::compute(&project.data)?;
    if previous["content-hash"] == hash {
        println!("composer.lock content hash is current.");
    } else if dry {
        println!("Would refresh the composer.lock content hash.");
    } else {
        previous["content-hash"] = hash.into();
        write_json(path, &previous)?;
        println!("Refreshed the composer.lock content hash.");
    }
    Ok(())
}

fn print_changes(previous: Option<&Lock>, packages: &[Package]) {
    let old: BTreeMap<_, _> = previous
        .into_iter()
        .flat_map(|lock| lock.packages.iter().chain(&lock.packages_dev))
        .map(|package| (package.name.as_str(), package))
        .collect();
    let new: BTreeMap<_, _> = packages
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();
    let names: BTreeSet<_> = old.keys().chain(new.keys()).copied().collect();
    let mut changes = 0;
    for name in names {
        match (old.get(name), new.get(name)) {
            (None, Some(to)) => println!("Added {name} {}", to.version),
            (Some(from), None) => println!("Removed {name} {}", from.version),
            (Some(from), Some(to))
                if from.version != to.version
                    || from.field("source")["reference"] != to.field("source")["reference"]
                    || from.field("dist")["reference"] != to.field("dist")["reference"] =>
            {
                println!("Updated {name} {} -> {}", from.version, to.version);
            }
            _ => continue,
        }
        changes += 1;
    }
    if changes == 0 {
        println!("No package version changes.");
    }
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let bytes = serde_json::to_string_pretty(value)? + "\n";
    write_file(path, bytes.as_bytes())
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("Composer file must have a parent directory.")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    if let Ok(metadata) = fs::metadata(path) {
        file.as_file().set_permissions(metadata.permissions())?;
    }
    file.write_all(bytes)?;
    file.persist(path)?;
    Ok(())
}
