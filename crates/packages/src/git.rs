//! List versions and export locked commits using the user's system git.

use crate::{Package, Project, archive, network::Network, store::Store, version::Version};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod remote;

use remote::Remote;

struct Mirror {
    path: PathBuf,
    remote: Remote,
    _guard: File,
}

impl Mirror {
    fn open(
        remote: Remote,
        store: &Store,
        network: &Network,
        offline: bool,
        refresh: bool,
    ) -> Result<Self> {
        let key = format!("{:x}", Sha256::digest(remote.url.as_bytes()));
        let path = store.directory.join("git").join(&key);
        let guard = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(store.directory.join("locks").join(format!("git-{key}")))?;
        guard.lock()?;
        if !path.is_dir() {
            if offline {
                bail!("Offline Git mirror is missing. Resolve or install online first.");
            }
            let staging = tempfile::tempdir_in(store.directory.join("git"))?;
            let clone = staging.path().join("mirror");
            let mut command = remote.command(network)?;
            command
                .args(["clone", "--mirror", "--no-hardlinks", "--", &remote.url])
                .arg(&clone);
            output(
                command,
                0,
                "Cannot clone Git repository. Check access and the Git URL.",
            )?;
            fs::rename(clone, &path)?;
        } else if refresh && !offline {
            let mut command = remote.command(network)?;
            command
                .arg("-C")
                .arg(&path)
                .args(["fetch", "--prune", "--prune-tags", "origin"]);
            output(
                command,
                0,
                "Cannot fetch Git repository. Check repository access.",
            )?;
        }
        Ok(Self {
            path,
            remote,
            _guard: guard,
        })
    }

    fn command(&self, network: &Network) -> Result<Command> {
        let mut command = self.remote.command(network)?;
        command.arg("-C").arg(&self.path);
        Ok(command)
    }
}

pub(crate) fn packages(
    project: &Project,
    address: &str,
    store: &Store,
    network: &Network,
    offline: bool,
) -> Result<Vec<Package>> {
    let remote = Remote::new(&project.directory, address, network)?;
    let mirror = Mirror::open(remote, store, network, offline, true)?;
    let mut command = mirror.command(network)?;
    command.args([
        "for-each-ref",
        "--format=%(refname)%09%(objectname)%09%(*objectname)",
        "refs/tags",
        "refs/heads",
    ]);
    let refs = output(command, 16 * 1024 * 1024, "Cannot list Git references.")?;
    let mut packages = Vec::new();
    for line in std::str::from_utf8(&refs)?.lines() {
        let mut fields = line.split('\t');
        let (Some(name), Some(object), Some(peeled)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let version = if let Some(tag) = name.strip_prefix("refs/tags/") {
            tag.to_owned()
        } else if let Some(branch) = name.strip_prefix("refs/heads/") {
            if Version::parse(&format!("{branch}.x-dev")).is_ok()
                && branch
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_digit())
            {
                format!("{branch}.x-dev")
            } else {
                format!("dev-{branch}")
            }
        } else {
            continue;
        };
        if !reference(object) || Version::parse(&version).is_err() {
            continue;
        }
        let commit = if reference(peeled) { peeled } else { object };
        let mut command = mirror.command(network)?;
        command.args(["show", &format!("{commit}:composer.json")]);
        let Some(bytes) = read_output(command, 1024 * 1024)? else {
            continue;
        };
        let Ok(mut manifest) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if !manifest.is_object() || manifest["name"].as_str().is_none() {
            continue;
        }
        manifest["version"] = version.into();
        manifest["source"] = json!({"type": "git", "url": address, "reference": commit});
        if let Some(data) = manifest.as_object_mut() {
            data.shift_remove("dist");
        }
        let package: Package = serde_json::from_value(manifest)?;
        package.validate()?;
        packages.push(package);
    }
    Ok(packages)
}

pub(crate) fn export(
    package: &Package,
    project: &Path,
    store: &Store,
    network: &Network,
    offline: bool,
    target: &Path,
) -> Result<()> {
    let source = package.field("source");
    if source["type"] != "git" {
        bail!(
            "{} has no dist archive or supported git source.",
            package.name
        );
    }
    let url = source["url"]
        .as_str()
        .context("Git source is missing its URL.")?;
    let commit = source["reference"]
        .as_str()
        .filter(|value| reference(value))
        .context("Locked git packages need a full commit ID.")?;
    let remote = Remote::new(project, url, network)?;
    let mirror = Mirror::open(remote, store, network, offline, false)?;
    let mut command = mirror.command(network)?;
    command.args(["archive", "--format=zip", commit]);
    let bytes = match read_output(command, crate::network::MAX_DOWNLOAD)? {
        Some(bytes) => bytes,
        None if !offline => {
            let mut fetch = mirror.command(network)?;
            fetch.args(["fetch", "origin", commit]);
            output(fetch, 0, "Cannot fetch the locked Git commit.")?;
            let mut archive = mirror.command(network)?;
            archive.args(["archive", "--format=zip", commit]);
            output(
                archive,
                crate::network::MAX_DOWNLOAD,
                "Cannot export the locked Git commit.",
            )?
        }
        None => bail!("Locked Git commit is missing from the offline mirror."),
    };
    archive::extract(&bytes, "zip", target)
}

fn reference(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn output(command: Command, limit: u64, failure: &str) -> Result<Vec<u8>> {
    read_output(command, limit)?.with_context(|| failure.to_owned())
}

fn read_output(mut command: Command, limit: u64) -> Result<Option<Vec<u8>>> {
    let stdout = if limit == 0 {
        Stdio::null()
    } else {
        Stdio::piped()
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::null())
        .spawn()
        .context("Cannot start system git.")?;
    let mut bytes = Vec::new();
    if let Some(output) = child.stdout.take() {
        let result = output.take(limit + 1).read_to_end(&mut bytes);
        if result.is_err() || bytes.len() as u64 > limit {
            let _ = child.kill();
            let _ = child.wait();
            result?;
            bail!("Git output exceeded its size limit.");
        }
    }
    Ok(child.wait()?.success().then_some(bytes))
}
