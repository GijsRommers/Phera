//! Compare actual package versions with published Packagist advisory ranges.
//!
//! `"config":{"audit":{"ignore":{"PKSA-…":"Accepted for this environment"}}}`
//! keeps a matching advisory visible without treating it as a failure.

use crate::version::{Constraint, Version};
use crate::{Lock, Options, Package, Project, installed, network::Network};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

#[derive(Default)]
pub struct AuditReport {
    active: BTreeMap<String, Vec<Value>>,
    ignored: BTreeMap<String, Vec<Value>>,
}

impl AuditReport {
    pub fn has_advisories(&self) -> bool {
        !self.active.is_empty()
    }

    pub fn json(&self) -> Result<String> {
        let entries = if self.active.is_empty() {
            json!([])
        } else {
            json!(self.active)
        };
        let mut report = json!({"advisories": entries});
        if !self.ignored.is_empty() {
            report["ignored-advisories"] = json!(self.ignored);
        }
        Ok(serde_json::to_string_pretty(&report)?)
    }

    pub fn table(&self) -> Result<String> {
        let mut output = self.summary();
        if !self.active.is_empty() || !self.ignored.is_empty() {
            output.push_str(
                "\n\nPackage | Advisory | Severity | Affected versions | Status | Title\n",
            );
            output.push_str("--- | --- | --- | --- | --- | ---\n");
        }
        for (status, advisories) in [("open", &self.active), ("ignored", &self.ignored)] {
            for (package, items) in advisories {
                for item in items {
                    let columns = [
                        package.as_str(),
                        field(item, "advisoryId"),
                        field(item, "severity"),
                        field(item, "affectedVersions"),
                        status,
                        field(item, "title"),
                    ];
                    let cells: Vec<_> = columns
                        .iter()
                        .map(|cell| {
                            cell.chars()
                                .filter(|c| !c.is_control())
                                .collect::<String>()
                                .replace('|', "\\|")
                        })
                        .collect();
                    writeln!(output, "{}", cells.join(" | "))?;
                }
            }
        }
        Ok(output.trim_end().to_owned())
    }

    fn summary(&self) -> String {
        let active: usize = self.active.values().map(Vec::len).sum();
        let ignored: usize = self.ignored.values().map(Vec::len).sum();
        match (active, ignored) {
            (0, 0) => "No security vulnerability advisories found.".into(),
            _ => format!(
                "Found {active} security advisories affecting {} packages ({ignored} ignored).",
                self.active.len()
            ),
        }
    }
}

pub fn check(directory: &Path, options: &Options) -> Result<AuditReport> {
    let project = Project::read(directory)?;
    if options.offline {
        bail!("Security advisories require online access.");
    }
    if !packagist_enabled(&project) {
        bail!("Packagist is disabled for this project.");
    }
    let packages = if options.frozen {
        Lock::read(directory)?.packages(options.no_dev)
    } else {
        installed::read::packages(&project, options.no_dev)
            .context("Use --locked to audit composer.lock.")?
    };
    query(&project, &packages)
}

pub(crate) fn summary(project: &Project, options: &Options, packages: &[Package]) {
    if !options.audit_summary {
        return;
    }
    if options.offline || !packagist_enabled(project) {
        eprintln!("Security audit skipped (Packagist disabled or offline).");
        return;
    }
    match query(project, packages) {
        Ok(report) => println!("{}", report.summary()),
        Err(error) => eprintln!("Security audit unavailable: {error:#}"),
    }
}

fn packagist_enabled(project: &Project) -> bool {
    let repos = &project.data["repositories"];
    let disabled = |entry: &Value| entry["packagist.org"] == false || entry["packagist"] == false;
    if disabled(repos) {
        return false;
    }
    let entries: Vec<_> = match repos {
        Value::Object(entries) => entries.values().collect(),
        Value::Array(entries) => entries.iter().collect(),
        _ => Vec::new(),
    };
    !entries.into_iter().any(disabled)
}

fn query(project: &Project, packages: &[Package]) -> Result<AuditReport> {
    let mut versions: BTreeMap<&str, Vec<Version>> = BTreeMap::new();
    for package in packages {
        package.validate()?;
        let mut version = Version::parse(&package.version)?;
        if version.is_branch() {
            let Some(alias) = package.field("extra")["branch-alias"][&package.version].as_str()
            else {
                continue;
            };
            version = Version::branch_alias(alias)?;
        }
        if version.normalized() == "9999999-dev" {
            continue;
        }
        versions.entry(&package.name).or_default().push(version);
    }
    let names: Vec<_> = versions.keys().copied().collect();
    let mut report = AuditReport::default();
    if names.is_empty() {
        return Ok(report);
    }
    let ignores = &project.data["config"]["audit"]["ignore"];
    let network = Network::new(project)?;
    for batch in names.chunks(500) {
        let response = network.packagist_advisories(batch)?;
        report.add(&versions, ignores, response)?;
    }
    Ok(report)
}

impl AuditReport {
    fn add(
        &mut self,
        versions: &BTreeMap<&str, Vec<Version>>,
        ignores: &Value,
        response: Value,
    ) -> Result<()> {
        let advisories = &response["advisories"];
        if advisories.as_array().is_some_and(Vec::is_empty) {
            return Ok(());
        }
        for (name, list) in advisories
            .as_object()
            .context("Missing Packagist advisories.")?
        {
            let Some(installed) = versions.get(name.as_str()) else {
                continue;
            };
            for entry in list
                .as_array()
                .context("Invalid Packagist advisory list.")?
            {
                let id = entry["advisoryId"]
                    .as_str()
                    .context("Advisory ID is missing.")?;
                let range = entry["affectedVersions"]
                    .as_str()
                    .context("Advisory range is missing.")?;
                let affected = Constraint::parse(range)
                    .with_context(|| format!("Invalid affected range for advisory {id}."))?;
                if !installed.iter().any(|version| affected.matches(version)) {
                    continue;
                }
                let mut entry = entry.clone();
                if let Some(reason) = ignore_reason(ignores, name, &entry) {
                    entry["ignoreReason"] = reason.map_or(Value::Null, |reason| reason.into());
                    self.ignored.entry(name.clone()).or_default().push(entry);
                } else {
                    self.active.entry(name.clone()).or_default().push(entry);
                }
            }
        }
        Ok(())
    }
}

fn ignore_reason<'a>(
    ignores: &'a Value,
    package: &str,
    advisory: &Value,
) -> Option<Option<&'a str>> {
    let mut ids = vec![
        package,
        field(advisory, "advisoryId"),
        field(advisory, "cve"),
    ];
    if let Some(sources) = advisory["sources"].as_array() {
        ids.extend(sources.iter().map(|source| field(source, "remoteId")));
    }
    for id in ids.into_iter().rev().filter(|id| !id.is_empty()) {
        if let Some(rule) = ignores.get(id) {
            if matches!(rule["apply"].as_str(), Some("block")) || rule["on-audit"] == false {
                continue;
            }
            return Some(rule.as_str().or_else(|| rule["reason"].as_str()));
        }
        if crate::project::strings(ignores).contains(&id) {
            return Some(None);
        }
    }
    None
}

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_ranges_and_matches_id_ignores_without_hiding_them() {
        let versions =
            BTreeMap::from([("example/package", vec![Version::parse("2.1.9").unwrap()])]);
        let response = json!({"advisories":{"example/package":[
            {"advisoryId":"PKSA-ignored","affectedVersions":">=1,<1.2 || >=2,<2.2"},
            {"advisoryId":"PKSA-open","affectedVersions":"2.1.*","severity":"medium"},
            {"advisoryId":"PKSA-unaffected","affectedVersions":">=2.2"},
        ]}});
        let mut report = AuditReport::default();
        report
            .add(&versions, &json!({"PKSA-ignored":"accepted"}), response)
            .unwrap();
        assert!(report.has_advisories());
        assert_eq!(report.active["example/package"].len(), 1);
        assert_eq!(
            report.ignored["example/package"][0]["ignoreReason"],
            "accepted"
        );
        assert!(!report.table().unwrap().contains("PKSA-unaffected"));
    }
}
