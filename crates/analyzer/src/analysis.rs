//! Body analysis: `function f() { echo $missing; }` reports a proven bad read.
//! Recursive nesting is bounded at 256; uncertain or deeper expressions are mixed.

use crate::{
    dependencies::Dependencies,
    diagnostic::Diagnostic,
    index::Index,
    names::Names,
    project::ParsedFile,
    rules::Finding,
    scope::{Definition, Scope, Symbol},
    types::ValueType,
};
use phera_ast::*;
use phera_source::Span;

mod arguments;
mod assignments;
mod atoms;
mod binding_statements;
mod bindings;
mod callable_signatures;
mod calls;
mod closure_binding;
mod coalesce_assignment;
mod composite;
mod control;
mod declarations;
mod deferred_declarations;
mod exceptions;
mod expressions;
mod finally;
mod intrinsics;
mod invocations;
mod logical;
mod loops;
mod match_expression;
mod member_expressions;
mod members;
mod not_supported;
mod refine;
mod reporting;
mod short_circuit;
mod statements;
mod switch;
mod write;

pub const MAX_DEPTH: usize = 256;

#[derive(Clone)]
pub struct Context {
    pub names: Names,
    pub known: std::collections::BTreeSet<Symbol>,
    pub class: Option<String>,
    pub this_class: Option<String>,
    pub object: bool,
    pub unbound_this: Option<Vec<crate::index::Location>>,
    pub returns: ValueType,
    pub declared_return: bool,
    pub by_reference: bool,
    pub function: String,
    pub scope_kind: ScopeKind,
    pub incomplete: bool,
}

pub struct Analyzer<'a> {
    pub index: &'a mut Index,
    pub dependencies: &'a mut Dependencies,
    pub file: &'a ParsedFile,
    pub level: u8,
    pub diagnostics: Vec<Diagnostic>,
    pub context: Context,
    pub depth: usize,
    pub checked_declarations: std::collections::BTreeSet<usize>,
    pub exception_scopes: Vec<Vec<Scope>>,
    pub return_scopes: Vec<Vec<Scope>>,
    pub controls: Vec<control::ControlFrame>,
    pub pending_binding: Option<(usize, closure_binding::ClosureBinding)>,
}

impl<'a> Analyzer<'a> {
    pub fn new(
        index: &'a mut Index,
        dependencies: &'a mut Dependencies,
        file: &'a ParsedFile,
        level: u8,
    ) -> Self {
        Self {
            index,
            dependencies,
            file,
            level,
            diagnostics: Vec::new(),
            depth: 0,
            checked_declarations: std::collections::BTreeSet::new(),
            exception_scopes: Vec::new(),
            return_scopes: Vec::new(),
            controls: Vec::new(),
            pending_binding: None,
            context: Context {
                names: Names::for_block(String::new(), &file.program.statements),
                known: std::collections::BTreeSet::new(),
                class: None,
                this_class: None,
                object: false,
                unbound_this: None,
                returns: ValueType::Mixed,
                declared_return: false,
                by_reference: false,
                function: "top level".into(),
                scope_kind: ScopeKind::Global,
                incomplete: false,
            },
        }
    }

    pub fn run(mut self) -> Vec<Diagnostic> {
        let mut scope = Scope::default();
        scope.define("argc", ValueType::Int);
        scope.define("argv", ValueType::Array);
        self.block(&self.file.program.statements, &mut scope);
        self.deferred_declarations();
        self.diagnostics
    }

    pub fn read(&mut self, name: &str, scope: &Scope, span: Span, quiet: bool) -> ValueType {
        if name == "this" {
            if !self.context.object && !quiet && scope.get(name).definition != Definition::Definite
            {
                if let Some(reads) = &mut self.context.unbound_this {
                    reads.push(crate::index::Location {
                        file: self.file.path.clone(),
                        line: self.file.lines.line(span.start),
                    });
                } else {
                    self.report(Finding::ThisContext, span);
                }
            }
            return self
                .context
                .this_class
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
        let special = name.text.to_ascii_lowercase();
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
        if let Some(class) = &resolved {
            self.dependencies.resolve(self.index, class);
        }
        if let Some(class) = &resolved
            && self.index.class(class).is_none()
            && !self
                .index
                .builtin_classes
                .contains(&class.to_ascii_lowercase())
            && !self
                .context
                .known
                .contains(&Symbol::Class(class.to_ascii_lowercase()))
            && report
        {
            self.report(Finding::UnknownClass(class.clone()), name.span);
        }
        resolved
    }

    pub fn class_expression(&mut self, expression: &Expression, scope: &mut Scope) -> ClassTarget {
        if let ExpressionKind::Name(name) = &expression.kind {
            let class = self.class_name(name, false);
            if let Some(class) = &class
                && self.index.class(class).is_none()
                && !self
                    .index
                    .builtin_classes
                    .contains(&class.to_ascii_lowercase())
                && !scope
                    .known
                    .contains(&Symbol::Class(class.to_ascii_lowercase()))
                && !self
                    .context
                    .known
                    .contains(&Symbol::Class(class.to_ascii_lowercase()))
            {
                self.report(Finding::UnknownClass(class.clone()), name.span);
            }
            class.map_or(ClassTarget::Unknown, ClassTarget::Known)
        } else if self.expression(expression, scope, false) == ValueType::Never {
            ClassTarget::Never
        } else {
            ClassTarget::Unknown
        }
    }
}

#[derive(Clone, Copy)]
pub enum Access {
    Checked,
    Inferred,
    Static,
    Forwarded,
    LateStatic { has_object: bool },
}

impl Access {
    pub fn object(target: &Expression) -> Self {
        if matches!(&target.kind, ExpressionKind::Variable(Variable::Named(n)) if n == "this") {
            Self::Checked
        } else {
            Self::Inferred
        }
    }
    pub fn static_call(self) -> bool {
        matches!(self, Self::Static | Self::LateStatic { has_object: false })
    }
    pub fn dynamic_receiver(self) -> bool {
        matches!(self, Self::Checked | Self::LateStatic { .. })
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Global,
    Function,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Evaluation {
    Completes,
    Never,
}

pub enum ClassTarget {
    Known(String),
    Unknown,
    Never,
}
impl ClassTarget {
    pub fn is_never(&self) -> bool {
        matches!(self, Self::Never)
    }
    pub fn name(self) -> Option<String> {
        if let Self::Known(name) = self {
            Some(name)
        } else {
            None
        }
    }
}
