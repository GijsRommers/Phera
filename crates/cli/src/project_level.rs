//! Choose the project's language level for one Phera invocation.
//!
//! `phera run --php 8.6 script.php` makes `phpversion()` report PHP 8.6.

use phera_analyzer::config::ProjectConfig;
use phera_source::LanguageLevel;
use std::path::Path;

pub(crate) fn resolve(
    specified: Option<LanguageLevel>,
    directory: &Path,
) -> Result<LanguageLevel, String> {
    if let Some(level) = specified {
        return Ok(level);
    }
    let config = ProjectConfig::find(directory)?;
    with_config(None, config.as_ref(), directory)
}

pub(crate) fn with_config(
    specified: Option<LanguageLevel>,
    config: Option<&ProjectConfig>,
    directory: &Path,
) -> Result<LanguageLevel, String> {
    if let Some(level) = specified {
        return Ok(level);
    }
    if let Some(config) = config
        && let Some(level) = config.php()?
    {
        return Ok(level);
    }
    let directory = directory
        .canonicalize()
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    for ancestor in directory.ancestors() {
        let path = ancestor.join("composer.json");
        match std::fs::read_to_string(&path) {
            Ok(source) => {
                return Ok(crate::composer_level::read(&path, &source)?.unwrap_or_default());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
    }
    Ok(LanguageLevel::default())
}

pub(crate) fn script_directory(script: &Path) -> &Path {
    script
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
