//! Body analysis: `function f() { echo $missing; }` reports a proven bad read.
//! Recursive nesting is bounded at 256; uncertain or deeper expressions are mixed.

use crate::{
    diagnostic::Diagnostic,
    index::Index,
    names::Names,
    project::ParsedFile,
    rules::{self, Finding},
    scope::{Definition, Scope, Symbol},
    types::ValueType,
};
use phera_ast::*;
use phera_source::Span;

mod arguments;
mod bindings;
mod calls;
mod declarations;
mod exceptions;
mod expressions;
mod intrinsics;
mod members;
mod refine;
mod statements;

pub const MAX_DEPTH: usize = 256;

#[derive(Clone)]
pub struct Context {
    pub names: Names,
    pub class: Option<String>,
    pub object: bool,
    pub returns: ValueType,
    pub declared_return: bool,
    pub function: String,
    pub incomplete: bool,
}

pub struct Analyzer<'a> {
    pub index: &'a Index,
    pub file: &'a ParsedFile,
    pub level: u8,
    pub diagnostics: Vec<Diagnostic>,
    pub context: Context,
    pub depth: usize,
    pub exception_scopes: Vec<Vec<Scope>>,
}

impl<'a> Analyzer<'a> {
    pub fn new(index: &'a Index, file: &'a ParsedFile, level: u8) -> Self {
        Self {
            index,
            file,
            level,
            diagnostics: Vec::new(),
            depth: 0,
            exception_scopes: Vec::new(),
            context: Context {
                names: Names::for_block(String::new(), &file.program.statements),
                class: None,
                object: false,
                returns: ValueType::Mixed,
                declared_return: false,
                function: "top level".into(),
                incomplete: false,
            },
        }
    }

    pub fn run(mut self) -> Vec<Diagnostic> {
        let mut scope = Scope::default();
        scope.define("argc", ValueType::Int);
        scope.define("argv", ValueType::Array);
        self.block(&self.file.program.statements, &mut scope);
        self.diagnostics
    }

    pub fn report(&mut self, finding: Finding, span: Span) {
        for rule in rules::all() {
            if rule.level() <= self.level
                && let Some(message) = rule.message(&finding)
            {
                self.diagnostics.push(Diagnostic {
                    file: self.file.path.clone(),
                    line: self.file.lines.line(span.start),
                    identifier: rule.identifier().into(),
                    message,
                });
            }
        }
    }

    pub fn read(&mut self, name: &str, scope: &Scope, span: Span, quiet: bool) -> ValueType {
        if name == "this" {
            if !self.context.object && !quiet {
                self.report(Finding::ThisContext, span);
            }
            return self
                .context
                .class
                .as_ref()
                .map_or(ValueType::Mixed, |class| ValueType::Class(class.clone()));
        }
        let variable = scope.get(name);
        if !quiet && !scope.dynamic {
            match variable.definition {
                Definition::Absent => self.report(Finding::UndefinedVariable(name.into()), span),
                Definition::Possible => self.report(Finding::PossibleVariable(name.into()), span),
                Definition::Definite | Definition::Unknown => {}
            }
        }
        variable.ty
    }

    pub fn class_name(&mut self, name: &Name, report: bool) -> Option<String> {
        let special = name.text.to_lowercase();
        let resolved = match special.as_str() {
            "self" | "static" => self.context.class.clone(),
            "parent" => self
                .context
                .class
                .as_ref()
                .and_then(|c| self.index.class(c))
                .and_then(|c| c.parents.first())
                .cloned(),
            _ => Some(self.context.names.resolve(name, UseKind::Class)[0].clone()),
        };
        if let Some(class) = &resolved
            && self.index.class(class).is_none()
            && report
        {
            self.report(Finding::UnknownClass(class.clone()), name.span);
        }
        resolved
    }

    pub fn class_expression(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
    ) -> Option<String> {
        if let ExpressionKind::Name(name) = &expression.kind {
            let class = self.class_name(name, false);
            if let Some(class) = &class
                && self.index.class(class).is_none()
                && !scope.known.contains(&Symbol::Class(class.to_lowercase()))
            {
                self.report(Finding::UnknownClass(class.clone()), name.span);
            }
            class
        } else {
            self.expression(expression, scope, false);
            None
        }
    }
}

#[derive(Clone, Copy)]
pub enum Access {
    Checked,
    Inferred,
    Static,
    Forwarded,
}

impl Access {
    pub fn object(target: &Expression) -> Self {
        if matches!(&target.kind, ExpressionKind::Variable(Variable::Named(n)) if n == "this") {
            Self::Checked
        } else {
            Self::Inferred
        }
    }
    pub fn reports_missing(self) -> bool {
        !matches!(self, Self::Inferred)
    }
}

#[derive(Clone, Copy)]
pub enum CallKind {
    Direct,
    Callable,
}

#[derive(Clone, Copy)]
pub enum PropertyAccess {
    Instance,
    Static,
}
