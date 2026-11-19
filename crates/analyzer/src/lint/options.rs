//! `phera lint src --rule unused-import --format json` selects one rule with JSON output.

use crate::composer::manifest::Manifest;
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

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum VariableStyle {
    #[default]
    Camel,
    Snake,
}

#[derive(Clone)]
pub(super) struct Thresholds {
    pub cyclomatic: usize,
    pub cognitive: usize,
    pub nesting: usize,
    pub function_lines: usize,
    pub class_lines: usize,
    pub parameters: usize,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            cyclomatic: 10,
            cognitive: 15,
            nesting: 4,
            function_lines: 80,
            class_lines: 500,
            parameters: 5,
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
    pub workers: usize,
    pub(super) manifest: Option<Manifest>,
    pub(super) variables: VariableStyle,
    pub(super) thresholds: Thresholds,
    strict_types: bool,
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
            workers: project::default_workers(),
            manifest: None,
            variables: VariableStyle::default(),
            thresholds: Thresholds::default(),
            strict_types: false,
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
        options.manifest = Manifest::discover(&options.paths);
        Ok(Command::Run(Box::new(options)))
    }

    pub fn read_config(&mut self, source: &str) -> Result<(), String> {
        config::read_tables(
            source,
            |table| {
                matches!(
                    table,
                    "lint"
                        | "lint.groups"
                        | "lint.rules"
                        | "lint.style"
                        | "lint.naming"
                        | "lint.complexity"
                )
            },
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
            ("lint.style", "strict_types") => self.strict_types = boolean(scalar()?)?,
            ("lint.naming", "variables") => {
                self.variables = match config::string(scalar()?)?.as_str() {
                    "camel" => VariableStyle::Camel,
                    "snake" => VariableStyle::Snake,
                    _ => {
                        return Err("phera.toml: lint.naming.variables needs camel or snake".into());
                    }
                };
            }
            ("lint.complexity", threshold) => {
                let limit = scalar()?
                    .parse::<usize>()
                    .map_err(|_| format!("phera.toml: {threshold} needs an integer threshold"))?;
                match threshold {
                    "cyclomatic" => self.thresholds.cyclomatic = limit,
                    "cognitive" => self.thresholds.cognitive = limit,
                    "nesting" => self.thresholds.nesting = limit,
                    "function_lines" => self.thresholds.function_lines = limit,
                    "class_lines" => self.thresholds.class_lines = limit,
                    "parameters" => self.thresholds.parameters = limit,
                    _ => {
                        return Err(format!(
                            "phera.toml: unknown complexity threshold: {threshold}"
                        ));
                    }
                }
            }
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
        if rule.name == "strict-types" && !self.strict_types && configured.is_none() {
            return None;
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

fn boolean(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("phera.toml: expected true or false".into()),
    }
}
