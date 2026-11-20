//! Run the same autoload and installed metadata probes under PHP 8.5 and Phera.

use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{command, run, run_output};

const PROBE: &str = r#"<?php
$vendor = $argv[1];
$loader = require $vendor . '/autoload.php';
$normalize = static function ($path) {
    $root = getcwd() . '/';
    if (realpath($path) === getcwd()) { return '.'; }
    return str_replace($root, '', str_replace('\\', '/', realpath($path) ?: $path));
};
$result = ['classes' => [], 'files' => [], 'psr4' => [], 'psr0' => []];
foreach ($loader->getClassMap() as $name => $path) {
    $result['classes'][$name] = $normalize($loader->findFile($name));
}
foreach (['Phera\DefinitelyMissing', 'NoSuchPheraClass', 'Composer\DefinitelyMissing'] as $name) {
    $result['classes'][$name] = $loader->findFile($name);
}
ksort($result['classes']);
foreach ($loader->getPrefixesPsr4() as $prefix => $paths) {
    $result['psr4'][$prefix] = array_map($normalize, $paths);
}
foreach ($loader->getPrefixes() as $prefix => $paths) {
    $result['psr0'][$prefix] = array_map($normalize, $paths);
}
ksort($result['psr4']);
ksort($result['psr0']);
foreach ($GLOBALS['__composer_autoload_files'] ?? [] as $key => $loaded) {
    $result['files'][] = $key;
}
echo "\nPHERA_PROBE_JSON=" . json_encode($result, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
"#;

pub(super) struct Php {
    binary: PathBuf,
    extensions: Vec<String>,
}

impl Php {
    pub(super) fn find(work: &Path) -> Result<Self> {
        let binary =
            phera_extension_api::testing::oracle::find_php().context("PHP 8.5 required")?;
        let output = run(
            command(&binary).args([
                "-n",
                "-r",
                "echo PHP_VERSION_ID, PHP_EOL, ini_get('extension_dir');",
            ]),
            work,
            "php-version",
        )?;
        let (version, directory) = output.split_once('\n').context("PHP version and modules")?;
        anyhow::ensure!(
            version.starts_with("805"),
            "Expected PHP 8.5, got {version}."
        );
        let extensions = [
            "phar",
            "iconv",
            "ctype",
            "openssl",
            "curl",
            "mbstring",
            "zip",
            "tokenizer",
            "dom",
            "simplexml",
            "xml",
            "xmlwriter",
        ]
        .into_iter()
        .filter(|name| Path::new(directory).join(format!("{name}.so")).is_file())
        .map(str::to_owned)
        .collect();
        Ok(Self { binary, extensions })
    }

    pub(super) fn command(&self) -> Command {
        let mut php = command(&self.binary);
        php.arg("-n");
        for extension in &self.extensions {
            php.args(["-d", &format!("extension={extension}")]);
        }
        php
    }

    pub(super) fn installed(
        &self,
        project: &Path,
        vendor: &Path,
        logs: &Path,
        name: &str,
    ) -> Result<Value> {
        let contents = run(
            self.command()
                .args([
                    "-r",
                    concat!(
                        "echo json_encode(require $argv[1], ",
                        "JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);",
                    ),
                ])
                .arg(vendor.join("composer/installed.php")),
            logs,
            name,
        )?;
        Ok(serde_json::from_str(
            &contents.replace(&*project.to_string_lossy(), "."),
        )?)
    }
}

pub(super) fn write_probe(work: &Path) -> Result<PathBuf> {
    let path = work.join("autoload-probe.php");
    fs::write(&path, PROBE)?;
    Ok(path)
}

pub(super) fn observe(
    command: &mut Command,
    probe: &Path,
    vendor: &Path,
    project: &Path,
    logs: &Path,
    label: &str,
) -> Result<Value> {
    let output = run_output(
        command.arg(probe).arg(vendor).current_dir(project),
        logs,
        label,
    )?;
    let normalize =
        |bytes| String::from_utf8_lossy(bytes).replace(&*project.to_string_lossy(), ".");
    let stdout = normalize(&output.stdout);
    let stderr = normalize(&output.stderr);
    if !output.status.success() {
        return Ok(
            json!({"status": output.status.code(), "output": stdout, "diagnostics": stderr}),
        );
    }
    let (prefix, payload) = stdout
        .rsplit_once("\nPHERA_PROBE_JSON=")
        .context("autoload payload")?;
    let result: Value = serde_json::from_str(payload)?;
    Ok(json!({"result": result, "output": prefix, "diagnostics": stderr}))
}
