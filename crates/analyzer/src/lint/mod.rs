//! Source linting without PHP expression types.
//!
//! ```php
//! use App\Unused;
//! $ports = ['http' => 80, 'http' => 8080];
//! ```
//!
//! Rules register a stable name, a group, examples and the nodes they inspect.
//! A single explicit work stack visits the file. A rule can defer decisions
//! such as unused imports until the namespace's references have been seen.
//! A fix is a set of byte edits. The fixer applies compatible sets, parses the
//! result before saving it and checks that the edits introduce no findings.

mod fix;
mod imports;
mod index;
mod locals;
mod members;
mod modernize;
mod options;
mod rule;
mod source;
mod suspicious;
mod traversal;

pub use options::{Command, FixMode, Language, Options};
pub use rule::{Group, Rule, catalog, find, rules};

use crate::{
    Report, baseline,
    diagnostic::{self, Diagnostic, Severity},
    names::Names,
    project, suppress,
};
use options::FixMode as Mode;
use phera_ast::{Statement, StatementKind};
use phera_source::{LineTable, Span};
use rule::{Check, Visit};
use source::Source;
use std::{collections::HashMap, path::Path, time::Instant};
use traversal::Node;

const MAX_FIX_PASSES: usize = 8;

struct Finding {
    diagnostic: Diagnostic,
    fix: Option<fix::Fix>,
}

#[derive(Default)]
struct Enabled {
    severities: HashMap<&'static str, Severity>,
    blocks: Vec<Check>,
    statements: Vec<Check>,
    expressions: Vec<Check>,
    classes: Vec<Check>,
    methods: Vec<Check>,
    namespaces: Vec<Check>,
    locals: bool,
    members: bool,
    member_references: bool,
}

impl Enabled {
    fn new(options: &Options) -> Self {
        let mut enabled = Self::default();
        for rule in rules() {
            let Some(severity) = options.severity(rule) else {
                continue;
            };
            enabled.severities.insert(rule.name, severity);
            if let Some(check) = rule.check {
                let checks = match rule.visit {
                    Visit::Block => &mut enabled.blocks,
                    Visit::Statement => &mut enabled.statements,
                    Visit::Expression => &mut enabled.expressions,
                    Visit::Class => &mut enabled.classes,
                    Visit::Method => &mut enabled.methods,
                    Visit::Namespace => &mut enabled.namespaces,
                    Visit::File => continue,
                };
                checks.push(check);
            }
        }
        enabled.locals = locals::RULES
            .iter()
            .any(|rule| enabled.severities.contains_key(rule.name));
        enabled.members = members::RULES
            .iter()
            .any(|rule| enabled.severities.contains_key(rule.name));
        enabled.member_references = [
            "unused-private-method",
            "unused-private-property",
            "unused-private-constant",
            "readonly-write-outside-constructor",
            "missing-parent-constructor",
        ]
        .iter()
        .any(|name| enabled.severities.contains_key(name));
        enabled
    }
}

#[derive(Default)]
struct RuleScope {
    in_finally: bool,
}

struct Context<'a> {
    file: &'a str,
    source: Source<'a>,
    lines: LineTable,
    enabled: &'a Enabled,
    language: Language,
    names: Names,
    index: Option<&'a crate::index::Index>,
    scope: RuleScope,
    findings: Vec<Finding>,
}

impl Context<'_> {
    fn report(&mut self, rule: &'static str, span: Span, message: impl Into<String>) {
        self.record(rule, self.lines.line(span.start), message.into(), None);
    }

    fn with_fix(
        &mut self,
        rule: &'static str,
        span: Span,
        message: impl Into<String>,
        fix: fix::Fix,
    ) {
        self.record(rule, self.lines.line(span.start), message.into(), Some(fix));
    }

    fn record(&mut self, rule: &'static str, line: usize, message: String, fix: Option<fix::Fix>) {
        if let Some(&severity) = self.enabled.severities.get(rule) {
            self.findings.push(Finding {
                diagnostic: Diagnostic {
                    file: self.file.into(),
                    line,
                    identifier: rule.into(),
                    message,
                    severity,
                },
                fix,
            });
        }
    }
}

pub fn run(options: &Options) -> Result<Report, String> {
    let start = Instant::now();
    let paths = project::discover(&options.paths, &options.exclude)?;
    let enabled = Enabled::new(options);
    let (index, vendor_files) = if [
        "missing-parent-constructor",
        "unreachable-catch",
        "override-attribute",
    ]
    .iter()
    .any(|name| enabled.severities.contains_key(name))
    {
        let (index, vendor_files) = index::load(options, &paths)?;
        (Some(index), vendor_files)
    } else {
        (None, 0)
    };
    let files = project::parallel_files(&paths, options.workers, |path| {
        lint_file(path, options, &enabled, index.as_ref())
    })?;
    let mut diagnostics: Vec<_> = files.into_iter().flatten().collect();
    diagnostics.sort();
    diagnostics.dedup();
    if let Some(path) = &options.generate_baseline {
        baseline::write(path, &diagnostics)?;
    } else if let Some(path) = &options.baseline {
        baseline::filter(path, &mut diagnostics)?;
    }
    Ok(Report {
        diagnostics,
        files: paths.len(),
        vendor_files,
        elapsed: start.elapsed(),
    })
}

pub fn render(report: &Report, format: diagnostic::Format) -> String {
    diagnostic::render_lint(report, format)
}

fn lint_file(
    path: &Path,
    options: &Options,
    enabled: &Enabled,
    index: Option<&crate::index::Index>,
) -> Result<Vec<Diagnostic>, String> {
    let original = project::read_source(path)?;
    let file = project::display_path(path);
    let (_, mut findings) = analyze(&file, &original, options, enabled, index);
    let mut previous: Option<Vec<u8>> = None;
    if options.fix != Mode::None {
        for _ in 0..MAX_FIX_PASSES {
            let current = previous.as_deref().unwrap_or(&original);
            let Some(next) = fix::apply(
                current,
                options.fix,
                findings.iter().filter_map(|f| f.fix.as_ref()),
            ) else {
                break;
            };
            let (valid, next_findings) = analyze(&file, &next, options, enabled, index);
            if !valid || introduces_findings(&findings, &next_findings) {
                break;
            }
            findings = next_findings;
            previous = Some(next);
        }
    }
    if let Some(fixed) = previous
        && fixed != original
    {
        if project::read_source(path)? != original {
            return Err(format!("{file}: source changed while linting"));
        }
        std::fs::write(path, fixed).map_err(|error| format!("{file}: {error}"))?;
    }
    Ok(findings
        .into_iter()
        .map(|finding| finding.diagnostic)
        .collect())
}

fn introduces_findings(before: &[Finding], after: &[Finding]) -> bool {
    let signature = |finding: &Finding| {
        (
            finding.diagnostic.identifier.clone(),
            finding.diagnostic.message.clone(),
        )
    };
    let mut counts = HashMap::new();
    for finding in before {
        *counts.entry(signature(finding)).or_insert(0usize) += 1;
    }
    after.iter().any(|finding| {
        let Some(count) = counts.get_mut(&signature(finding)) else {
            return true;
        };
        if *count == 0 {
            return true;
        }
        *count -= 1;
        false
    })
}

fn analyze(
    file: &str,
    source: &[u8],
    options: &Options,
    enabled: &Enabled,
    index: Option<&crate::index::Index>,
) -> (bool, Vec<Finding>) {
    let mut context = Context {
        file,
        source: Source::new(source),
        lines: LineTable::from_bytes(source),
        enabled,
        language: options.language,
        names: Names::default(),
        index,
        scope: RuleScope::default(),
        findings: Vec::new(),
    };
    match phera_parser::parse_bytes_on_current_stack(source) {
        Ok(program) => {
            if !enabled.statements.is_empty()
                || !enabled.expressions.is_empty()
                || !enabled.blocks.is_empty()
                || !enabled.classes.is_empty()
                || !enabled.methods.is_empty()
                || !enabled.namespaces.is_empty()
                || enabled.locals
                || enabled.members
                || enabled.severities.contains_key("unused-import")
            {
                lint_namespace(&mut context, String::new(), &program.statements);
                for statement in &program.statements {
                    if let StatementKind::Namespace {
                        name, statements, ..
                    } = &statement.kind
                    {
                        lint_namespace(
                            &mut context,
                            name.as_ref().map_or_else(String::new, |n| n.text.clone()),
                            statements,
                        );
                    }
                }
            }
            context.filter_suppressions();
            (true, context.findings)
        }
        Err(error) => {
            let (line, message) = error.php_diagnostic(source, &context.lines);
            let message = if message.starts_with("unexpected ") {
                format!("syntax error, {message}")
            } else {
                message
            };
            context.record("syntax-error", line, message, None);
            (false, context.findings)
        }
    }
}

fn lint_namespace(context: &mut Context<'_>, namespace: String, statements: &[Statement]) {
    let enabled = context.enabled;
    context.names = Names::for_block(namespace, statements);
    let mut imports = imports::NamespaceImports::new(
        statements,
        enabled.severities.contains_key("unused-import"),
    );
    let mut locals = enabled.locals.then(locals::Locals::default);
    let mut members = enabled.members.then(members::Members::default);
    let mut classes = Vec::new();
    traversal::visit(statements, |node, scope| {
        if let Some(members) = &mut members {
            if let Node::Class(class) = node {
                classes.push(class);
            }
            members.observe(context, node, scope);
        }
        if let Some(locals) = &mut locals {
            locals.observe(node, scope);
        }
        context.scope = RuleScope {
            in_finally: scope.finally,
        };
        if let Node::Name(name, kind) = node {
            imports.reference(name, kind);
        }
        let checks = match node {
            Node::Block(_) => &enabled.blocks,
            Node::Statement(_) => &enabled.statements,
            Node::Expression(..) => &enabled.expressions,
            Node::Class(_) => &enabled.classes,
            Node::Method(_) => &enabled.methods,
            _ => return,
        };
        for check in checks {
            check(context, node);
        }
    });
    imports.report(context, statements);
    if let Some(locals) = locals {
        locals.report(context);
    }
    if let Some(members) = members {
        for class in classes {
            members.report(context, class);
        }
    }
    for check in &enabled.namespaces {
        check(context, Node::Block(statements));
    }
}

impl Context<'_> {
    fn filter_suppressions(&mut self) {
        const PREFIX: &[u8] = b"phera-ignore";
        if !self
            .source
            .bytes
            .windows(PREFIX.len())
            .any(|part| part == PREFIX)
        {
            return;
        }
        let mut directives =
            suppress::lint_directives(self.source.bytes, self.source.tokens(), &self.lines);
        self.findings
            .retain(|finding| !suppress::is_lint_suppressed(&finding.diagnostic, &mut directives));
        for directive in directives {
            if !directive.used
                && rule::find(directive.name)
                    .is_none_or(|rule| self.enabled.severities.contains_key(rule.name))
            {
                self.record(
                    "unused-suppression",
                    directive.line,
                    format!("Unused suppression for '{}'.", directive.name),
                    None,
                );
            }
        }
    }
}
