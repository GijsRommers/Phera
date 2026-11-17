//! Scan PHP declarations with Phera's parser on all cores.
//!
//! `namespace App; enum Status {}` adds `App\Status` to the optimized class map.

use anyhow::{Context, Result};
use phera_ast::{Statement, StatementKind};
use rayon::prelude::*;
use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) struct Source {
    directory: PathBuf,
    exclusions: Vec<Regex>,
    psr: Option<(String, bool)>,
}

impl Source {
    pub(super) fn new(
        base: &Path,
        path: &str,
        exclusions: &[String],
        psr: Option<(String, bool)>,
    ) -> Result<Self> {
        let exclusions = exclusions
            .iter()
            .map(|pattern| {
                let escaped = regex::escape(pattern.trim_matches('/'))
                    .replace("\\*\\*", ".+?")
                    .replace("\\*", "[^/]+?");
                Regex::new(&format!(
                    "^{}/{}($|/)",
                    regex::escape(&base.to_string_lossy()),
                    escaped
                ))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Self {
            directory: base.join(crate::paths::relative(path)?),
            exclusions,
            psr,
        })
    }
}

pub(super) fn classes(sources: &[Source], vendor: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let mut files = Vec::new();
    for source in sources {
        for path in glob::glob(&source.directory.to_string_lossy())? {
            let path = path?;
            let walker = walkdir::WalkDir::new(&path)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|entry| entry.path() != vendor);
            for entry in walker {
                let entry = entry?;
                if !entry.file_type().is_file() {
                    continue;
                }
                if !matches!(
                    entry.path().extension().and_then(|ext| ext.to_str()),
                    Some("php" | "inc" | "hh")
                ) {
                    continue;
                }
                if source
                    .exclusions
                    .iter()
                    .any(|pattern| pattern.is_match(&entry.path().to_string_lossy()))
                {
                    continue;
                }
                files.push((entry.into_path(), source));
            }
        }
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .stack_size(phera_parser::PARSER_STACK_BYTES)
        .build()?;
    let discovered: Vec<Vec<(String, PathBuf)>> = pool.install(|| {
        files
            .par_iter()
            .map(|(path, source)| -> Result<_> {
                let text = fs::read_to_string(path)
                    .with_context(|| format!("Cannot read PHP file {}.", path.display()))?;
                let program = phera_parser::parse_on_current_stack(&text)
                    .map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))?;
                let mut names = Vec::new();
                declarations(&program.statements, "", &mut names);
                Ok(names
                    .into_iter()
                    .filter(|name| compliant(name, path, source))
                    .map(|name| (name, path.clone()))
                    .collect())
            })
            .collect::<Result<_>>()
    })?;
    let mut classes = BTreeMap::new();
    for declarations in discovered {
        for (name, path) in declarations {
            classes.entry(name).or_insert(path);
        }
    }
    Ok(classes)
}

fn compliant(name: &str, path: &Path, source: &Source) -> bool {
    let Some((prefix, psr4)) = &source.psr else {
        return true;
    };
    let Some(suffix) = name.strip_prefix(prefix) else {
        return false;
    };
    let relative = if *psr4 {
        suffix.replace('\\', "/")
    } else {
        let (namespace, class) = name.rsplit_once('\\').unwrap_or(("", name));
        let class = class.replace('_', "/");
        if namespace.is_empty() {
            class
        } else {
            format!("{}/{class}", namespace.replace('\\', "/"))
        }
    };
    source.directory.join(format!("{relative}.php")) == path
}

fn declarations(statements: &[Statement], namespace: &str, names: &mut Vec<String>) {
    for statement in statements {
        match &statement.kind {
            StatementKind::Namespace {
                name, statements, ..
            } => {
                declarations(
                    statements,
                    name.as_ref().map_or("", |name| &name.text),
                    names,
                );
            }
            StatementKind::Class(class) => {
                if let Some(name) = &class.name {
                    names.push(if namespace.is_empty() {
                        name.name.clone()
                    } else {
                        format!("{namespace}\\{}", name.name)
                    });
                }
            }
            StatementKind::Block(body)
            | StatementKind::While { body, .. }
            | StatementKind::DoWhile { body, .. }
            | StatementKind::For { body, .. }
            | StatementKind::Foreach { body, .. } => declarations(body, namespace, names),
            StatementKind::If {
                then, otherwise, ..
            } => {
                declarations(then, namespace, names);
                if let Some(body) = otherwise {
                    declarations(body, namespace, names);
                }
            }
            StatementKind::Try {
                body,
                catches,
                finally,
            } => {
                declarations(body, namespace, names);
                for catch in catches {
                    declarations(&catch.body, namespace, names);
                }
                if let Some(body) = finally {
                    declarations(body, namespace, names);
                }
            }
            StatementKind::Switch { cases, .. } => {
                for case in cases {
                    declarations(&case.body, namespace, names);
                }
            }
            StatementKind::Declare {
                body: Some(body), ..
            } => declarations(body, namespace, names),
            StatementKind::Function(function) => declarations(&function.body, namespace, names),
            _ => {}
        }
    }
}
