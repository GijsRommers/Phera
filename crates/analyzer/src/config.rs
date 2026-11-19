//! Checker settings: `[check] level = 1` and `paths = ["src"]` in `phera.toml`.
//! Only the documented check table is interpreted; other tables are left alone.
//! For example, `new Missing` is checked at the configured level for these paths.

use crate::diagnostic::Format;
use phera_source::LanguageLevel;
use std::path::Path;

mod project_config;
pub use project_config::ProjectConfig;
use project_config::string;

#[derive(Clone)]
pub struct Options {
    pub php: LanguageLevel,
    pub level: u8,
    pub paths: Vec<String>,
    pub scan: Vec<String>,
    pub exclude: Vec<String>,
    pub baseline: Option<String>,
    pub generate_baseline: Option<String>,
    pub format: Format,
    pub no_progress: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            php: LanguageLevel::default(),
            level: 1,
            paths: Vec::new(),
            exclude: Vec::new(),
            scan: Vec::new(),
            baseline: None,
            generate_baseline: None,
            format: Format::Text,
            no_progress: false,
        }
    }
}

impl Options {
    pub fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let config = ProjectConfig::find(Path::new("."))?;
        Self::parse_with_config(arguments, config.as_ref())
    }

    pub fn parse_with_config(
        arguments: impl Iterator<Item = String>,
        config: Option<&ProjectConfig>,
    ) -> Result<Self, String> {
        let mut options = Self::default();
        if let Some(config) = config {
            for setting in config.settings("check") {
                options.setting(&setting.key, &setting.values, setting.array)?;
            }
        }
        let mut arguments = arguments.peekable();
        let mut paths = Vec::new();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--" => {
                    paths.extend(arguments);
                    break;
                }
                "--level" => {
                    options.level = level(&arguments.next().ok_or("--level needs 0 or 1")?)?
                }
                "--format" => {
                    options.format = match arguments.next().as_deref() {
                        Some("text") => Format::Text,
                        Some("json") => Format::Json,
                        Some("github") => Format::Github,
                        _ => return Err("--format needs text, json or github".into()),
                    }
                }
                "--no-progress" => options.no_progress = true,
                "--generate-baseline" => {
                    options.generate_baseline =
                        Some(arguments.next().ok_or("--generate-baseline needs a file")?)
                }
                value if value.starts_with('-') => {
                    return Err(format!("Unknown check option: {value}"));
                }
                _ => paths.push(argument),
            }
        }
        if !paths.is_empty() {
            options.paths = paths;
        }
        if options.paths.is_empty() {
            return Err("check needs paths or [check].paths in phera.toml".into());
        }
        Ok(options)
    }

    fn setting(&mut self, key: &str, values: &[String], array: bool) -> Result<(), String> {
        let scalar = || {
            values
                .first()
                .ok_or_else(|| format!("phera.toml: missing {key}"))
        };
        match key {
            "level" if !array => self.level = level(scalar()?)?,
            "paths" if array => {
                self.paths = values.iter().map(|s| string(s)).collect::<Result<_, _>>()?
            }
            "scan" if array => {
                self.scan = values.iter().map(|s| string(s)).collect::<Result<_, _>>()?
            }
            "exclude" if array => {
                self.exclude = values.iter().map(|s| string(s)).collect::<Result<_, _>>()?
            }
            "baseline" if !array => self.baseline = Some(string(scalar()?)?),
            "format" if !array => {
                self.format = match string(scalar()?)?.as_str() {
                    "text" => Format::Text,
                    "json" => Format::Json,
                    "github" => Format::Github,
                    _ => return Err("phera.toml: format must be text, json or github".into()),
                }
            }
            "no_progress" | "no-progress" if !array => {
                self.no_progress = match scalar()?.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => return Err("phera.toml: no_progress must be true or false".into()),
                }
            }
            _ => {
                return Err(format!(
                    "phera.toml: unsupported check setting or value: {key}"
                ));
            }
        }
        Ok(())
    }
}

fn level(value: &str) -> Result<u8, String> {
    match value {
        "0" => Ok(0),
        "1" => Ok(1),
        _ => Err("Supported check levels are 0 and 1".into()),
    }
}
