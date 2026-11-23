//! Recover the exact manifests, locks and source revisions used by PR #15.

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use super::{command, run};

const DRAFT: &str = "79d628d99532c2b6b7267521cf0b2f801cebfbb0";

pub(super) struct Fixture {
    pub name: String,
    pub manifest: Vec<u8>,
    pub lock: Vec<u8>,
    pub source: Source,
}

#[derive(Deserialize)]
pub(super) struct Source {
    dist: String,
    reference: String,
    pub root_version: String,
}

pub(super) fn fixtures(work: &Path) -> Result<Vec<Fixture>> {
    let pinned_archive = work.join("draft-fixtures.tar.gz");
    if !pinned_archive.is_file() {
        let download = work.join("draft-fixtures.part");
        run(
            command("curl")
                .args(["--fail", "--silent", "--show-error", "--location"])
                .args(["--proto", "=https", "--max-time", "120", "--output"])
                .arg(&download)
                .arg(format!(
                    "https://codeload.github.com/GijsRommers/Phera/tar.gz/{DRAFT}"
                )),
            work,
            "download-corpus",
        )?;
        fs::rename(download, &pinned_archive)?;
    }
    let decoder = GzDecoder::new(File::open(pinned_archive)?);
    let mut archive = tar::Archive::new(decoder);
    let mut corpus = BTreeMap::<String, BTreeMap<String, Vec<u8>>>::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let Some(relative) = path.to_str().and_then(|path| {
            path.strip_prefix(&format!("Phera-{DRAFT}/crates/packages/tests/fixtures/"))
        }) else {
            continue;
        };
        let Some((fixture, filename)) = relative.split_once('/') else {
            continue;
        };
        if !matches!(
            filename,
            "composer.json" | "composer.lock" | "provenance.json"
        ) {
            continue;
        }
        let fixture = fixture.to_owned();
        let filename = filename.to_owned();
        let mut bytes = Vec::new();
        (&mut entry).take(4 * 1024 * 1024).read_to_end(&mut bytes)?;
        corpus.entry(fixture).or_default().insert(filename, bytes);
    }
    if corpus.len() != 25 {
        bail!(
            "Expected 25 original Composer fixtures, found {}.",
            corpus.len()
        );
    }
    corpus
        .into_iter()
        .map(|(name, mut files)| {
            let mut read = |file| files.remove(file).with_context(|| format!("{name}/{file}"));
            let manifest = read("composer.json")?;
            let lock = read("composer.lock")?;
            let source = serde_json::from_slice(&read("provenance.json")?)?;
            Ok(Fixture {
                name,
                manifest,
                lock,
                source,
            })
        })
        .collect()
}

impl Fixture {
    pub(super) fn prepare(&self, root: &Path, logs: &Path) -> Result<()> {
        fs::create_dir(root)?;
        let Some((repo, _)) = self
            .source
            .dist
            .strip_prefix("https://api.github.com/repos/")
            .and_then(|path| path.split_once("/zipball/"))
        else {
            bail!("Unsupported fixture source for {}.", self.name);
        };
        let mut git = command("git");
        git.current_dir(root).args(["init", "--quiet"]);
        run(&mut git, logs, "git-init")?;
        let url = format!("https://github.com/{repo}.git");
        let mut git = command("git");
        git.current_dir(root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args([
                "fetch",
                "--quiet",
                "--depth=1",
                "--",
                &url,
                &self.source.reference,
            ]);
        run(&mut git, logs, "git-fetch")?;
        run(
            command("git")
                .current_dir(root)
                .args(["-c", "core.hooksPath=/dev/null"])
                .args(["checkout", "--quiet", "--detach", "FETCH_HEAD"]),
            logs,
            "git-checkout",
        )?;
        fs::remove_dir_all(root.join(".git"))?;
        fs::write(root.join("composer.json"), &self.manifest)?;
        fs::write(root.join("composer.lock"), &self.lock)?;
        Ok(())
    }
}
