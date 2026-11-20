//! `phera run --debug app.php` and `[debug]` in the project configuration.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use phera_dotenv::Environment;
use phera_runtime::DebugConfig;

pub(crate) fn project_root(document_root: &Path) -> &Path {
    let is_project = |path: &Path| {
        path.join("composer.json").is_file()
            || path.join("phera.toml").is_file()
            || path.join("vendor/composer").is_dir()
    };
    document_root
        .ancestors()
        .find(|path| is_project(path))
        .unwrap_or(document_root)
}

pub(crate) fn read_table(project: &Path) -> Result<(toml::Table, Option<PathBuf>), String> {
    let path = project.join("phera.toml");
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok((toml::Table::new(), None)),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let table = source
        .parse()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok((table, path.canonicalize().ok()))
}

pub(crate) fn configure(
    force: bool,
    table: &toml::Table,
    ini: &[(String, String)],
    environment: &Environment,
) -> Result<Option<DebugConfig>, String> {
    let mut settings = Vec::new();
    for (name, value) in table.iter().chain(
        table
            .get("ini")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flatten(),
    ) {
        if name.starts_with("xdebug.") {
            push_setting(&mut settings, name, value)?;
        }
    }
    for section in ["xdebug", "debug"] {
        if let Some(table) = table.get(section).and_then(toml::Value::as_table) {
            for (name, value) in table {
                if section == "debug" && name == "enabled" {
                    match value.as_bool() {
                        Some(true) => settings.push(("xdebug.mode".into(), "debug".into())),
                        Some(false) => settings.push(("xdebug.mode".into(), "off".into())),
                        None => return Err("debug.enabled must be true or false".into()),
                    }
                } else {
                    push_setting(&mut settings, &format!("xdebug.{name}"), value)?;
                }
            }
        }
    }
    let read = |name: &[u8]| {
        environment
            .get(name)
            .and_then(|value| std::str::from_utf8(value).ok())
    };
    DebugConfig::from_settings(
        force,
        &settings,
        ini,
        read(b"XDEBUG_MODE"),
        read(b"XDEBUG_CONFIG"),
    )
}

fn push_setting(
    settings: &mut Vec<(String, String)>,
    name: &str,
    value: &toml::Value,
) -> Result<(), String> {
    let value = match value {
        toml::Value::String(value) => value.clone(),
        toml::Value::Integer(value) => value.to_string(),
        _ => return Err(format!("{name} must be a string or integer")),
    };
    settings.push((name.to_owned(), value));
    Ok(())
}
