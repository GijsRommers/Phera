//! `phera lint src --rule unused-import --format json` selects one rule with JSON output.

use crate::{
    config,
    diagnostic::{Format, Severity},
    project,
};
use std::collections::{HashMap, HashSet};

use super::rule::{self, DefaultStatus, Group, Rule};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FixMode {
    #[default]
    None,
    Safe,
    All,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    Php85,
    Php86,
}

impl Language {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "8.5" => Ok(Self::Php85),
            "8.6" => Ok(Self::Php86),
            _ => Err("--php needs 8.5 or 8.6".into()),
        }
    }
}

#[derive(Clone)]
pub struct Options {
    pub paths: Vec<String>,
    pub exclude: Vec<String>,
    pub baseline: Option<String>,
    pub generate_baseline: Option<String>,
    pub format: Format,
    pub fix: FixMode,
    pub language: Language,
    pub workers: usize,
    select: Vec<&'static Rule>,
    groups: Vec<Group>,
    skip: HashSet<&'static str>,
    settings: HashMap<&'static str, Option<Severity>>,
    group_settings: HashMap<&'static str, bool>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            paths: vec![".".into()],
            exclude: Vec::new(),
            baseline: None,
            generate_baseline: None,
            format: Format::Text,
            fix: FixMode::None,
            language: Language::default(),
            workers: project::default_workers(),
            select: Vec::new(),
            groups: Vec::new(),
            skip: HashSet::new(),
            settings: HashMap::new(),
            group_settings: HashMap::new(),
        }
    }
}

pub enum Command {
    Run(Box<Options>),
    Explain(&'static Rule),
    List,
}

impl Options {
    pub fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Command, String> {
        let mut options = Self::default();
        let mut paths = Vec::new();
        let mut language = None;
        match std::fs::read_to_string("phera.toml") {
            Ok(source) => options.read_config(&source)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("phera.toml: {error}")),
        }
        while let Some(argument) = arguments.next() {
            let mut value = || {
                arguments
                    .next()
                    .ok_or_else(|| format!("{argument} needs a value"))
            };
            match argument.as_str() {
                "--" => {
                    paths.extend(arguments);
                    break;
                }
                "--fix" if options.fix != FixMode::All => options.fix = FixMode::Safe,
                "--fix" => {}
                "--fix-unsafe" => options.fix = FixMode::All,
                "--format" => options.format = format(&value()?)?,
                "--php" => language = Some(Language::parse(&value()?)?),
                "--rule" => options.select.push(named(&value()?)?),
                "--only" => options.groups.push(Group::parse(&value()?)?),
                "--skip" => {
                    options.skip.insert(named(&value()?)?.name);
                }
                "--explain" => return Ok(Command::Explain(named(&value()?)?)),
                "--list-rules" => return Ok(Command::List),
                "--generate-baseline" => options.generate_baseline = Some(value()?),
                "--threads" => {
                    options.workers = value()?
                        .parse::<usize>()
                        .ok()
                        .filter(|&n| n > 0)
                        .ok_or("--threads needs a positive integer")?;
                }
                value if value.starts_with('-') => {
                    return Err(format!("Unknown lint option: {value}"));
                }
                _ => paths.push(argument),
            }
        }
        if !paths.is_empty() {
            options.paths = paths;
        }
        if let Some(language) = language {
            options.language = language;
        }
        Ok(Command::Run(Box::new(options)))
    }

    pub fn read_config(&mut self, source: &str) -> Result<(), String> {
        config::read_tables(
            source,
            |table| matches!(table, "lint" | "lint.groups" | "lint.rules" | "runtime"),
            |table, key, values, array| self.setting(table, key, &values, array),
        )
    }

    fn setting(
        &mut self,
        table: &str,
        key: &str,
        values: &[String],
        array: bool,
    ) -> Result<(), String> {
        let scalar = || {
            values
                .first()
                .filter(|_| !array)
                .map(String::as_str)
                .ok_or_else(|| format!("phera.toml: {table}.{key} needs a scalar"))
        };
        match (table, key) {
            ("runtime", "php") => self.language = Language::parse(&config::string(scalar()?)?)?,
            ("runtime", _) => {}
            ("lint", "exclude") if array => {
                self.exclude = values
                    .iter()
                    .map(|value| config::string(value))
                    .collect::<Result<_, _>>()?;
            }
            ("lint", "baseline") => self.baseline = Some(config::string(scalar()?)?),
            ("lint", "format") => self.format = format(&config::string(scalar()?)?)?,
            ("lint.groups", group) => {
                let enabled = match scalar()? {
                    "true" => true,
                    "false" => false,
                    _ => return Err(format!("phera.toml: {table}.{key} needs true or false")),
                };
                self.group_settings
                    .insert(Group::parse(group)?.name(), enabled);
            }
            ("lint.rules", name) => {
                let severity = match config::string(scalar()?)?.as_str() {
                    "error" => Some(Severity::Error),
                    "warn" => Some(Severity::Warn),
                    "off" => None,
                    _ => {
                        return Err(format!(
                            "phera.toml: {table}.{key} needs error, warn or off"
                        ));
                    }
                };
                self.settings.insert(named(name)?.name, severity);
            }
            _ => return Err(format!("phera.toml: unsupported lint setting: {key}")),
        }
        Ok(())
    }

    pub(super) fn severity(&self, rule: &Rule) -> Option<Severity> {
        if self.skip.contains(rule.name) {
            return None;
        }
        let configured = self.settings.get(rule.name).copied();
        let explicitly_selected = self
            .select
            .iter()
            .any(|selected| selected.name == rule.name);
        if explicitly_selected {
            return Some(configured.flatten().unwrap_or_default());
        }
        if (!self.select.is_empty() || !self.groups.is_empty()) && rule.group != Group::Syntax {
            return self
                .groups
                .contains(&rule.group)
                .then_some(configured.unwrap_or(Some(Severity::Error)))
                .flatten();
        }
        if let Some(severity) = configured {
            return severity;
        }
        self.group_settings
            .get(rule.group.name())
            .copied()
            .unwrap_or(matches!(rule.default, DefaultStatus::Enabled))
            .then_some(Severity::Error)
    }
}

fn named(name: &str) -> Result<&'static Rule, String> {
    rule::find(name).ok_or_else(|| format!("Unknown lint rule: {name}"))
}

fn format(value: &str) -> Result<Format, String> {
    match value {
        "text" => Ok(Format::Text),
        "json" => Ok(Format::Json),
        "github" => Ok(Format::Github),
        _ => Err("--format needs text, json or github".into()),
    }
}
