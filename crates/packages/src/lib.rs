//! Composer-compatible packages, independent of the PHP runtime.
//!
//! `install(&project, &options)` installs an existing `composer.lock` without resolution.

mod archive;
mod autoload;
mod binaries;
mod cleanup;
mod commands;
mod content_hash;
mod install_paths;
mod installed;
mod lock_writer;
mod network;
mod path_install;
mod paths;
mod php;
mod platform;
mod plugins;
mod project;
mod registry;
mod scripts;
mod solver;
mod store;
mod update;
pub mod version;

pub use commands::{add, add_many, outdated, remove, remove_many, why};
pub use project::{Lock, Options, Package, Project};

use anyhow::{Context, Result, bail};
use install_paths::InstallPaths;
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use std::time::Instant;

/// Install the exact locked packages and generate a standalone PHP autoloader.
pub fn install(directory: &Path, options: &Options) -> Result<()> {
    if !directory.join("composer.lock").is_file() && !options.frozen {
        return update(directory, options);
    }
    install_locked(directory, options, false, &mut scripts::Runner::default())
}

fn install_locked(
    directory: &Path,
    options: &Options,
    updating: bool,
    runner: &mut scripts::Runner,
) -> Result<()> {
    let started = Instant::now();
    let project = Project::read(directory)?;
    let lock = Lock::read(directory)?;
    if lock.content_hash != content_hash::compute(&project.data)? {
        if options.frozen {
            bail!("composer.lock is out of date. Run `phera update` before frozen installation.");
        }
        eprintln!("Warning: composer.lock is out of date; installing its exact versions.");
    }
    platform::validate(&project, &lock, options)?;
    project.validate_output_paths()?;
    let packages = lock.packages(options.no_dev);
    plugins::validate(&project, options, &packages)?;
    let paths = InstallPaths::new(&project, options, &packages)?;
    let previous = cleanup::previous(&project)?;
    paths.validate_replacement(&project, &packages, &previous)?;
    if !updating {
        runner.event(&project, options, "pre-install-cmd")?;
    }
    materialise_packages(&project, &packages, options, &paths)?;
    cleanup::remove_stale(&project, &previous, &paths)?;
    installed::write(&project, &lock, options, &paths)?;
    binaries::write(&project, &packages, &paths)?;
    plugins::before_autoload(&project, options, &packages)?;
    runner.event(&project, options, "pre-autoload-dump")?;
    autoload::generate(&project, &packages, options, &paths)
        .context("Could not generate autoload files; correct the reported PHP file and retry.")?;
    plugins::post_autoload(&project, options, &packages)?;
    runner.event(&project, options, "post-autoload-dump")?;
    plugins::post_install(&project, options, &packages, &paths)?;
    runner.event(
        &project,
        options,
        if updating {
            "post-update-cmd"
        } else {
            "post-install-cmd"
        },
    )?;
    plugins::report(&project, options, &packages)?;
    println!(
        "Installed {} packages in {:.2}s.",
        packages.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn materialise_packages(
    project: &Project,
    packages: &[Package],
    options: &Options,
    paths: &InstallPaths,
) -> Result<()> {
    if packages.windows(2).any(|pair| pair[0].name == pair[1].name) {
        bail!("composer.lock contains duplicate package names. Run `phera update`.");
    }
    let cache = store::Store::open(options.cache_dir.as_deref())?;
    let network = network::Network::new(project)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(16.min(std::thread::available_parallelism()?.get() * 2))
        .build()?;
    let prepared = pool.install(|| {
        packages
            .par_iter()
            .filter(|package| package.kind() != "metapackage")
            .map(|package| {
                let source = cache
                    .prepare(package, &network, options.offline, &project.directory)
                    .with_context(|| {
                        format!("Cannot install {} {}.", package.name, package.version)
                    })?;
                Ok((package, source))
            })
            .collect::<Result<Vec<_>>>()
    })?;
    let vendor = project.vendor_dir()?;
    fs::create_dir_all(&vendor)?;
    pool.install(|| {
        prepared.par_iter().try_for_each(|(package, source)| {
            let destination = paths.destination(package)?;
            let method = if path_install::link(project, package, destination)? {
                "live path repository link"
            } else {
                store::materialise(source, destination, package.field("target-dir").as_str())?
            };
            if options.verbose {
                eprintln!("  {} {}: {method}", package.name, package.version);
            }
            Ok(())
        })
    })
}

/// Resolve composer.json through PubGrub, write a Composer lock and install it.
pub fn update(directory: &Path, options: &Options) -> Result<()> {
    update_selected(directory, &[], options)
}

/// Resolve selected packages while keeping the other locked versions fixed.
pub fn update_selected(directory: &Path, names: &[String], options: &Options) -> Result<()> {
    update::run(Project::read(directory)?, names, None, options)
}

/// Rebuild the PHP autoloader from the installed lock file.
pub fn dump_autoload(directory: &Path, options: &Options) -> Result<()> {
    let project = Project::read(directory)?;
    let lock = Lock::read(directory)?;
    let mut options = options.clone();
    let installed = project.vendor_dir()?.join("composer/installed.json");
    if let Ok(bytes) = fs::read(installed)
        && let Ok(record) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && record["dev"] == false
    {
        options.no_dev = true;
    }
    let mut runner = scripts::Runner::default();
    let packages = lock.packages(options.no_dev);
    plugins::validate(&project, &options, &packages)?;
    let paths = InstallPaths::new(&project, &options, &packages)?;
    runner.event(&project, &options, "pre-autoload-dump")?;
    autoload::generate(&project, &packages, &options, &paths)?;
    plugins::post_autoload(&project, &options, &packages)?;
    runner.event(&project, &options, "post-autoload-dump")
}
