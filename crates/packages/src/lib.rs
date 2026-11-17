//! Composer-compatible packages, independent of the PHP runtime.
//!
//! `install(&project, &options)` installs an existing `composer.lock` without resolution.

mod archive;
mod autoload;
mod binaries;
mod cleanup;
mod commands;
mod content_hash;
mod installed;
mod lock_writer;
mod network;
mod paths;
mod php;
mod project;
mod registry;
mod scripts;
mod solver;
mod store;
pub mod version;

pub use commands::{add, outdated, remove, why};
pub use project::{Lock, Options, Package, Project};

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use std::fs;
use std::path::Path;

/// Install the exact locked packages and generate a standalone PHP autoloader.
pub fn install(directory: &Path, options: &Options) -> Result<()> {
    if !directory.join("composer.lock").is_file() && !options.frozen {
        return update(directory, options);
    }
    install_locked(directory, options, false)
}

fn install_locked(directory: &Path, options: &Options, updating: bool) -> Result<()> {
    let project = Project::read(directory)?;
    let lock = Lock::read(directory)?;
    if lock.content_hash != content_hash::compute(&project.data)? {
        if options.frozen {
            bail!("composer.lock is out of date. Run `phera update` before frozen installation.");
        }
        eprintln!("Warning: composer.lock is out of date; installing its exact versions.");
    }
    if !updating {
        scripts::event(&project, options, "pre-install-cmd")?;
    }
    let vendor = project.vendor_dir()?;
    fs::create_dir_all(&vendor)?;
    let cache = store::Store::open(options.cache_dir.as_deref())?;
    let network = network::Network::new(&project.directory)?;
    let packages = lock.packages(options.no_dev);
    let previous = cleanup::previous(&project)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(16.min(std::thread::available_parallelism()?.get() * 2))
        .build()?;
    pool.install(|| {
        packages.par_iter().try_for_each(|package| -> Result<()> {
            package.validate()?;
            if package.kind() == "metapackage" {
                return Ok(());
            }
            let source = cache.prepare(package, &network, options.offline, &project.directory)?;
            let destination = paths::inside(&vendor, &package.name)?;
            let method = store::materialise(&source, &destination)?;
            if options.verbose {
                eprintln!("  {} {}: {method}", package.name, package.version);
            }
            Ok(())
        })
    })?;
    cleanup::remove_stale(&project, &previous, &packages)?;
    installed::write(&project, &lock, options)?;
    binaries::write(&project, &packages)?;
    scripts::event(&project, options, "pre-autoload-dump")?;
    autoload::generate(&project, &packages, options)
        .context("Could not generate autoload files; correct the reported PHP file and retry.")?;
    scripts::event(&project, options, "post-autoload-dump")?;
    scripts::event(
        &project,
        options,
        if updating {
            "post-update-cmd"
        } else {
            "post-install-cmd"
        },
    )?;
    for package in &packages {
        if package.kind() == "composer-plugin" {
            eprintln!(
                "Skipped Composer plugin {} (plugins are not executed).",
                package.name
            );
        }
    }
    println!("Installed {} packages.", packages.len());
    Ok(())
}

/// Resolve composer.json through PubGrub, write a Composer lock and install it.
pub fn update(directory: &Path, options: &Options) -> Result<()> {
    let project = Project::read(directory)?;
    scripts::event(&project, options, "pre-update-cmd")?;
    let registry = registry::Registry::new(&project, options)?;
    let packages = solver::solve(&project, options, &registry)?;
    lock_writer::write(&project, options, &packages)?;
    if options.no_install {
        return Ok(());
    }
    install_locked(directory, options, true)
}

/// Rebuild the PHP autoloader from the installed lock file.
pub fn dump_autoload(directory: &Path, options: &Options) -> Result<()> {
    let project = Project::read(directory)?;
    let lock = Lock::read(directory)?;
    scripts::event(&project, options, "pre-autoload-dump")?;
    autoload::generate(&project, &lock.packages(options.no_dev), options)?;
    scripts::event(&project, options, "post-autoload-dump")
}
