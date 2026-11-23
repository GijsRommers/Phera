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

mod complexity;
mod fix;
mod foreach_reference;
mod imports;
mod index;
mod locals;
mod members;
mod modernize;
mod naming;
mod options;
mod pipeline;
mod rule;
mod source;
mod style;
mod suspicious;
mod traversal;

pub use options::{Command, FixMode, Options};
pub use pipeline::run;
pub use rule::{Group, Rule, catalog, find, rules};

use crate::{
    Report,
    diagnostic::{self, Diagnostic, Severity},
    names::Names,
    suppress,
};
use phera_ast::{Statement, StatementKind};
use phera_source::{LineTable, Span};
use rule::{Check, Visit};
use source::Source;
use std::collections::HashMap;
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
    file_checks: Vec<Check>,
    types: Vec<Check>,
    parameters: Vec<Check>,
    namespaces: Vec<Check>,
    locals: bool,
    members: bool,
    member_references: bool,
    complexity: bool,
    foreach_references: bool,
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
                    Visit::File => &mut enabled.file_checks,
                    Visit::Type => &mut enabled.types,
                    Visit::List => {
                        enabled.parameters.push(check);
                        &mut enabled.expressions
                    }
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
        enabled.complexity = complexity::RULES.iter().any(|rule| {
            rule.visit == Visit::Namespace && enabled.severities.contains_key(rule.name)
        });
        enabled.foreach_references = enabled.severities.contains_key(foreach_reference::NAME);
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
    options: &'a Options,
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

pub fn render(report: &Report, format: diagnostic::Format) -> String {
    diagnostic::render_lint(report, format)
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
        options,
        names: Names::default(),
        index,
        scope: RuleScope::default(),
        findings: Vec::new(),
    };
    let (parsed, _) = phera_parser::parse_file_on_current_stack(source, true);
    match parsed {
        Ok(program) => {
            if !enabled.statements.is_empty()
                || !enabled.expressions.is_empty()
                || !enabled.blocks.is_empty()
                || !enabled.classes.is_empty()
                || !enabled.methods.is_empty()
                || !enabled.types.is_empty()
                || !enabled.namespaces.is_empty()
                || enabled.locals
                || enabled.complexity
                || enabled.members
                || enabled.foreach_references
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
            for check in &enabled.file_checks {
                check(&mut context, Node::Block(&program.statements));
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
    let mut references = enabled
        .foreach_references
        .then(foreach_reference::References::default);
    let mut members = enabled.members.then(members::Members::default);
    let mut complexity = enabled.complexity.then(complexity::Counts::default);
    let mut classes = Vec::new();
    traversal::visit(statements, |node, scope| {
        if let Some(complexity) = &mut complexity {
            complexity.observe(context, node, scope);
        }
        if let Some(members) = &mut members {
            if let Node::Class(class) = node {
                classes.push(class);
            }
            members.observe(context, node, scope);
        }
        if let Some(references) = &mut references {
            references.observe(node, scope);
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
            Node::Type(_) => &enabled.types,
            Node::Parameters(_) => &enabled.parameters,
            _ => return,
        };
        for check in checks {
            check(context, node);
        }
    });
    imports.report(context, statements);
    if let Some(complexity) = complexity {
        complexity.report(context);
    }
    if let Some(references) = references {
        references.report(context);
    }
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

#[cfg(test)]
mod verification;
