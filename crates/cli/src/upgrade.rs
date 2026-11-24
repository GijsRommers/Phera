//! Install a verified release next to the running executable.

mod http;

use std::cmp::Ordering;
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use flate2::read::GzDecoder;
use rustix::fs::{Access, access};
use semver::Version;
use sha2::{Digest, Sha256};
use url::Url;

const RELEASES: &str = "https://github.com/GijsRommers/Phera/releases/";
const LATEST: &str = "https://api.github.com/repos/GijsRommers/Phera/releases/latest";
const METADATA_LIMIT: usize = 1024 * 1024;
const ARCHIVE_LIMIT: usize = 128 * 1024 * 1024;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub(crate) const HELP: &str = "\
Usage: phera upgrade [OPTIONS]

Update Phera from a release.

Options:
  --check                   Report the available version without installing it.
  --version VERSION         Install this release, even if it is older.
  -h, --help                Print this help.
";

#[derive(Default)]
pub(crate) struct Options {
    check: bool,
    version: Option<Version>,
}

impl Options {
    pub(crate) fn parse(
        mut args: impl Iterator<Item = String>,
    ) -> std::result::Result<Self, String> {
        let mut options = Self::default();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--check" => options.check = true,
                "--version" => {
                    let version = args.next().ok_or("--version needs a version.")?;
                    options.version = Some(parse_version(&version)?);
                }
                _ => return Err(format!("Unknown upgrade option: {arg}")),
            }
        }
        Ok(options)
    }
}

pub(crate) fn command(options: Options) -> ExitCode {
    let result = (|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(upgrade(options))
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Phera upgrade: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn upgrade(options: Options) -> Result<()> {
    let current = parse_version(env!("CARGO_PKG_VERSION"))?;
    let platform = platform(
        std::env::consts::OS,
        std::env::consts::ARCH,
        cfg!(target_env = "gnu"),
    )
    .ok_or("Upgrades are available for macOS and glibc Linux on arm64 or x86_64.")?;
    let mirror = std::env::var("PHERA_RELEASE_BASE_URL").ok();
    let base = match &mirror {
        Some(base) => http::https_url(&format!("{}/", base.trim_end_matches('/')))?,
        None => Url::parse(RELEASES)?,
    };
    let client = http::Client::new().await?;
    let explicit = options.version.is_some();
    let target = match options.version {
        Some(version) => version,
        None => {
            let latest = match mirror {
                Some(_) => base.join("latest")?,
                None => Url::parse(LATEST)?,
            };
            let json = client.get(latest, METADATA_LIMIT).await?;
            let metadata: serde_json::Value = serde_json::from_slice(&json)?;
            let tag = metadata["tag_name"]
                .as_str()
                .ok_or("The latest release has no version tag.")?;
            parse_version(tag)?
        }
    };
    if !needs_upgrade(&current, &target, explicit) {
        println!("Phera {current} is already up to date.");
        return Ok(());
    }

    let archive_name = format!("phera-{target}-{platform}.tar.gz");
    let release = base.join(&format!("download/v{target}/"))?;
    let checksums = release.join("SHA256SUMS")?;
    if options.check {
        if explicit {
            let sums = client.get(checksums, METADATA_LIMIT).await?;
            archive_checksum(str::from_utf8(&sums)?, &archive_name)?;
        }
        println!("Installed Phera {current}. Available release: Phera {target}.");
        return Ok(());
    }

    let executable = fs::canonicalize(std::env::current_exe()?)?;
    let directory = executable
        .parent()
        .ok_or("Phera has no install directory.")?;
    check_install_location(&executable, directory)?;
    let directory_lock = File::open(directory)?;
    directory_lock.try_lock().map_err(|error| {
        format!(
            "Could not lock {} for upgrade: {error}",
            directory.display()
        )
    })?;

    let sums = client.get(checksums, METADATA_LIMIT).await?;
    let archive = client
        .get(release.join(&archive_name)?, ARCHIVE_LIMIT)
        .await?;
    verify_checksum(str::from_utf8(&sums)?, &archive_name, &archive)?;
    replace_executable(&executable, directory, &archive)?;
    println!("Updated Phera {current} to {target}.");
    Ok(())
}

fn parse_version(version: &str) -> std::result::Result<Version, String> {
    Version::parse(version.strip_prefix('v').unwrap_or(version))
        .map_err(|error| format!("Invalid Phera version {version:?}: {error}"))
}

fn needs_upgrade(current: &Version, target: &Version, explicit: bool) -> bool {
    if explicit {
        target != current
    } else {
        target.cmp_precedence(current) == Ordering::Greater
    }
}

fn platform(os: &str, arch: &str, glibc: bool) -> Option<&'static str> {
    match (os, arch, glibc) {
        ("macos", "aarch64", _) => Some("macos-arm64"),
        ("macos", "x86_64", _) => Some("macos-x86_64"),
        ("linux", "aarch64", true) => Some("linux-arm64"),
        ("linux", "x86_64", true) => Some("linux-x86_64"),
        _ => None,
    }
}

fn verify_checksum(sums: &str, name: &str, archive: &[u8]) -> Result<()> {
    let digest = archive_checksum(sums, name)?;
    if !digest.eq_ignore_ascii_case(&base16ct::lower::encode_string(&Sha256::digest(archive))) {
        return Err(format!("SHA-256 verification failed for {name}.").into());
    }
    Ok(())
}

fn archive_checksum<'a>(sums: &'a str, name: &str) -> Result<&'a str> {
    let mut expected = sums.lines().filter_map(|line| {
        let (digest, filename) = line.split_once(char::is_whitespace)?;
        let filename = filename.trim_start();
        let filename = filename.strip_prefix('*').unwrap_or(filename);
        (filename == name).then_some(digest)
    });
    let digest = expected
        .next()
        .ok_or_else(|| format!("No SHA-256 for {name}."))?;
    if expected.next().is_some()
        || digest.len() != 64
        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!("Invalid SHA-256 entry for {name}.").into());
    }
    Ok(digest)
}

fn check_install_location(executable: &Path, directory: &Path) -> Result<()> {
    if executable
        .file_name()
        .is_some_and(|name| name == "phera.old")
    {
        return Err("Run the installed Phera binary to upgrade it.".into());
    }
    let package_prefixes = [
        "/opt/homebrew",
        "/home/linuxbrew/.linuxbrew",
        "/opt/local",
        "/nix/store",
        "/snap",
        "/usr/bin",
        "/usr/sbin",
    ];
    let homebrew = std::env::var_os("HOMEBREW_PREFIX").map(PathBuf::from);
    if package_prefixes
        .iter()
        .any(|prefix| executable.starts_with(prefix))
        || executable
            .components()
            .any(|part| part.as_os_str() == "Cellar")
        || homebrew
            .is_some_and(|prefix| !prefix.as_os_str().is_empty() && executable.starts_with(prefix))
    {
        return Err(format!(
            "{} is managed by a package manager. Update Phera through that package manager.",
            executable.display()
        )
        .into());
    }
    if executable.metadata()?.permissions().readonly()
        || access(executable, Access::WRITE_OK).is_err()
        || access(directory, Access::WRITE_OK | Access::EXEC_OK).is_err()
    {
        return Err(format!("Phera cannot write to {} or its directory. Install Phera in a directory you can write to.", executable.display()).into());
    }
    if directory.join("phera.old").is_dir() {
        return Err(format!(
            "{} is a directory. Move it before upgrading.",
            directory.join("phera.old").display()
        )
        .into());
    }
    Ok(())
}

fn replace_executable(executable: &Path, directory: &Path, archive: &[u8]) -> Result<()> {
    let stage = tempfile::Builder::new()
        .prefix(".phera-upgrade-")
        .tempdir_in(directory)?;
    let replacement = stage.path().join("phera");
    let backup = stage.path().join("previous");
    let old = directory.join("phera.old");
    let mut file = File::create(&replacement)?;
    extract_binary(archive, &mut file)?;
    let mode = executable.metadata()?.permissions().mode() & 0o777;
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    file.sync_all()?;

    fs::hard_link(executable, &backup)?;
    fs::rename(replacement, executable)?;
    if let Err(error) = fs::rename(&backup, &old) {
        if fs::rename(&backup, executable).is_err() {
            let saved = stage.keep().join("previous");
            return Err(format!(
                "Could not save the previous binary at {}: {error}. It is still at {}.",
                old.display(),
                saved.display()
            )
            .into());
        }
        return Err(format!(
            "Could not save the previous binary at {}: {error}. Restored the original executable.",
            old.display()
        )
        .into());
    }
    Ok(())
}

fn extract_binary(archive: &[u8], file: &mut File) -> Result<()> {
    // Use one regular file from the archive and choose its destination ourselves.
    let decoder = GzDecoder::new(archive).take(2 * ARCHIVE_LIMIT as u64);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries()?.raw(true) {
        let mut entry = entry?;
        if entry.path_bytes().as_ref() == b"phera" && entry.header().entry_type().is_file() {
            let size = entry.size();
            if size == 0 || size > ARCHIVE_LIMIT as u64 || io::copy(&mut entry, file)? != size {
                return Err("The release archive has an invalid binary size.".into());
            }
            return Ok(());
        }
    }
    Err("The release archive has no Phera binary.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_use_semver_precedence_unless_explicit() {
        let version = |value| parse_version(value).unwrap();
        assert!(needs_upgrade(&version("1.9.0"), &version("v1.10.0"), false));
        assert!(needs_upgrade(
            &version("1.0.0-rc.2"),
            &version("1.0.0"),
            false
        ));
        assert!(!needs_upgrade(
            &version("1.0.0"),
            &version("1.0.0+build"),
            false
        ));
        assert!(!needs_upgrade(&version("1.0.0"), &version("0.9.0"), false));
        assert!(needs_upgrade(&version("1.0.0"), &version("0.9.0"), true));
        assert!(parse_version("../latest").is_err());
    }

    #[test]
    fn archive_platform_names_match_the_release_matrix() {
        assert_eq!(platform("macos", "aarch64", false), Some("macos-arm64"));
        assert_eq!(platform("macos", "x86_64", false), Some("macos-x86_64"));
        assert_eq!(platform("linux", "aarch64", true), Some("linux-arm64"));
        assert_eq!(platform("linux", "x86_64", true), Some("linux-x86_64"));
        assert_eq!(platform("linux", "x86_64", false), None);
        assert_eq!(platform("windows", "x86_64", false), None);
    }

    #[test]
    fn checksums_match_archive_name_and_bytes() {
        let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let sums = format!("{digest}  other.tar.gz\n{digest} *phera.tar.gz\n");
        assert!(verify_checksum(&sums, "phera.tar.gz", b"abc").is_ok());
        assert!(verify_checksum(&sums, "phera.tar.gz", b"bad").is_err());
        assert!(verify_checksum(&sums, "missing.tar.gz", b"abc").is_err());
        assert!(verify_checksum(&sums.repeat(2), "phera.tar.gz", b"abc").is_err());
    }
}
