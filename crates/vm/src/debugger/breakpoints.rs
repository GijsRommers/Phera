//! `breakpoint_set -t line -f file:///app.php -n 7 -i 2` registers a source stop.

use std::collections::BTreeMap;
use std::fmt::Write;

use quick_xml::escape::escape;

use super::protocol::Command;

#[derive(Default)]
pub(super) struct Breakpoints {
    next: u64,
    entries: BTreeMap<u64, Breakpoint>,
}

#[derive(Clone)]
pub(super) struct Breakpoint {
    pub(super) id: u64,
    pub(super) kind: Kind,
    pub(super) enabled: bool,
    pub(super) temporary: bool,
    pub(super) expression: Option<Vec<u8>>,
    pub(super) hits: u64,
    pub(super) hit_value: u64,
    comparison: Option<HitCondition>,
}

#[derive(Clone)]
pub(super) enum Kind {
    Line {
        file: String,
        requested: u32,
        resolved: Option<u32>,
        conditional: bool,
    },
    Call(String),
    Return(String),
    Exception(String),
}

#[derive(Clone, Copy)]
enum HitCondition {
    AtLeast,
    Equal,
    Multiple,
}

impl Breakpoints {
    pub(super) fn command(
        &mut self,
        command: &Command,
        file: &str,
        lines: impl Fn(&str, u32) -> Option<u32>,
        include_resolution: bool,
    ) -> String {
        match command.name.as_str() {
            "breakpoint_list" => command.response(
                "",
                &self
                    .entries
                    .values()
                    .map(|item| item.xml(include_resolution))
                    .collect::<String>(),
            ),
            "breakpoint_set" => {
                self.next += 1;
                let Some(mut item) = Breakpoint::parse(self.next, command, file) else {
                    return command.error(3, "invalid breakpoint options");
                };
                item.resolve(lines);
                let mut attrs = format!(r#" id="{}""#, item.id);
                if include_resolution {
                    let _ = write!(attrs, r#" resolved="{}""#, item.resolution());
                }
                self.entries.insert(item.id, item);
                command.response(&attrs, "")
            }
            _ => {
                let Some(id) = command.number(b'd') else {
                    return command.error(3, "missing breakpoint id");
                };
                let Some(item) = self.entries.get_mut(&id) else {
                    return command.error(205, "no such breakpoint");
                };
                match command.name.as_str() {
                    "breakpoint_get" => command.response("", &item.xml(include_resolution)),
                    "breakpoint_remove" => {
                        let body = item.xml(include_resolution);
                        self.entries.remove(&id);
                        command.response("", &body)
                    }
                    "breakpoint_update" => {
                        let mut updated = item.clone();
                        if updated.update(command).is_none() {
                            return command.error(3, "invalid breakpoint options");
                        }
                        updated.resolve(lines);
                        *item = updated;
                        command.response("", &item.xml(include_resolution))
                    }
                    _ => command.error(4, "unimplemented command"),
                }
            }
        }
    }

    pub(super) fn latest_resolution(&self) -> Option<String> {
        let item = self.entries.get(&self.next)?;
        (matches!(item.kind, Kind::Line { .. }) && item.resolution() == "resolved")
            .then(|| item.xml(true))
    }

    pub(super) fn resolve_file(
        &mut self,
        file: &str,
        lines: impl Fn(&str, u32) -> Option<u32>,
    ) -> Vec<String> {
        let mut resolved = Vec::new();
        for item in self.entries.values_mut() {
            if matches!(&item.kind, Kind::Line { file: name, resolved: None, .. } if name == file) {
                item.resolve(&lines);
                if item.resolution() == "resolved" {
                    resolved.push(item.xml(true));
                }
            }
        }
        resolved
    }

    pub(super) fn has_exceptions(&self) -> bool {
        self.entries
            .values()
            .any(|item| item.enabled && matches!(item.kind, Kind::Exception(_)))
    }

    pub(super) fn candidates(
        &mut self,
        matches: impl Fn(&Kind) -> bool,
    ) -> Vec<(u64, Option<Vec<u8>>)> {
        self.entries
            .values_mut()
            .filter(|item| item.enabled && matches(&item.kind))
            .map(|item| (item.id, item.expression.clone()))
            .collect()
    }

    pub(super) fn hit(&mut self, id: u64, resolved: bool) -> Option<String> {
        let item = self.entries.get_mut(&id)?;
        item.hits = item.hits.saturating_add(1);
        let stop = item.hit_value == 0
            || match item.comparison.unwrap_or(HitCondition::AtLeast) {
                HitCondition::AtLeast => item.hits >= item.hit_value,
                HitCondition::Equal => item.hits == item.hit_value,
                HitCondition::Multiple => item.hits.is_multiple_of(item.hit_value),
            };
        let details = stop.then(|| item.xml(resolved));
        if stop && item.temporary {
            self.entries.remove(&id);
        }
        details
    }
}

impl Breakpoint {
    fn parse(id: u64, command: &Command, file: &str) -> Option<Self> {
        let kind = match command.get(b't')? {
            kind @ ("line" | "conditional") => Kind::Line {
                file: command.get(b'f').unwrap_or(file).to_owned(),
                requested: command.number(b'n').filter(|line| *line > 0)?,
                resolved: None,
                conditional: kind == "conditional",
            },
            kind @ ("call" | "return") => {
                let name = command.get(b'm')?;
                let name = match command.get(b'a') {
                    Some(class) => format!("{class}::{name}"),
                    None => name.to_owned(),
                };
                if kind == "call" {
                    Kind::Call(name)
                } else {
                    Kind::Return(name)
                }
            }
            "exception" => Kind::Exception(command.get(b'x')?.to_owned()),
            _ => return None,
        };
        let mut item = Self {
            id,
            kind,
            enabled: true,
            temporary: false,
            expression: None,
            hits: 0,
            hit_value: 0,
            comparison: None,
        };
        item.update(command)?;
        Some(item)
    }

    fn update(&mut self, command: &Command) -> Option<()> {
        if let Some(state) = command.get(b's') {
            self.enabled = match state {
                "enabled" => true,
                "disabled" => false,
                _ => return None,
            };
        }
        if let Some(hit) = command.get(b'h') {
            self.hit_value = hit.parse().ok()?;
        }
        if let Some(condition) = command.get(b'o') {
            self.comparison = Some(match condition {
                ">=" => HitCondition::AtLeast,
                "==" => HitCondition::Equal,
                "%" => HitCondition::Multiple,
                _ => return None,
            });
        }
        if let Some(temporary) = command.get(b'r') {
            self.temporary = temporary == "1";
        }
        if let Some(data) = command.data() {
            self.expression = (!data.is_empty()).then(|| data.to_vec());
        }
        if let Kind::Line {
            requested,
            resolved,
            ..
        } = &mut self.kind
            && let Some(line) = command.number(b'n')
        {
            *requested = line;
            *resolved = None;
        }
        Some(())
    }

    fn resolve(&mut self, lines: impl Fn(&str, u32) -> Option<u32>) {
        if let Kind::Line {
            file,
            requested,
            resolved,
            ..
        } = &mut self.kind
        {
            *resolved = lines(file, *requested);
        }
    }

    fn resolution(&self) -> &'static str {
        match self.kind {
            Kind::Line { resolved: None, .. } => "unresolved",
            _ => "resolved",
        }
    }

    fn state(&self) -> &'static str {
        if self.enabled { "enabled" } else { "disabled" }
    }

    fn xml(&self, include_resolution: bool) -> String {
        let mut element = format!(
            r#"<breakpoint id="{}" type="{}" state="{}" hit_count="{}" hit_value="{}""#,
            self.id,
            self.kind.label(),
            self.state(),
            self.hits,
            self.hit_value
        );
        if let Some(comparison) = self.comparison {
            let comparison = match comparison {
                HitCondition::AtLeast => "&gt;=",
                HitCondition::Equal => "==",
                HitCondition::Multiple => "%",
            };
            let _ = write!(element, r#" hit_condition="{comparison}""#);
        }
        match &self.kind {
            Kind::Line {
                file,
                requested,
                resolved,
                ..
            } => {
                let _ = write!(
                    element,
                    r#" filename="{}" lineno="{}""#,
                    escape(file),
                    resolved.unwrap_or(*requested)
                );
            }
            Kind::Call(name) | Kind::Return(name) => {
                if let Some((class, method)) = name.split_once("::") {
                    let _ = write!(
                        element,
                        r#" class="{}" function="{}""#,
                        escape(class),
                        escape(method)
                    );
                } else {
                    let _ = write!(element, r#" function="{}""#, escape(name));
                }
            }
            Kind::Exception(name) => {
                let _ = write!(element, r#" exception="{}""#, escape(name));
            }
        }
        if include_resolution {
            let _ = write!(element, r#" resolved="{}""#, self.resolution());
        }
        element.push_str("></breakpoint>");
        element
    }
}

impl Kind {
    fn label(&self) -> &'static str {
        match self {
            Self::Line {
                conditional: false, ..
            } => "line",
            Self::Line {
                conditional: true, ..
            } => "conditional",
            Self::Call(_) => "call",
            Self::Return(_) => "return",
            Self::Exception(_) => "exception",
        }
    }
}
