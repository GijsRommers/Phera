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

mod argument_contracts;
mod arguments;
mod array_key_guards;
mod array_merge;
mod array_property_append;
mod array_size;
mod array_sum;
mod arrays;
mod assignments;
mod atoms;
mod attributes;
mod binding_statements;
mod bindings;
mod builtin_facts;
mod call_assertions;
mod callable_signatures;
mod callback_parameters;
mod calls;
mod chain_value;
mod closure_binding;
mod closure_effects;
mod coalesce_assignment;
mod coalesce_guards;
mod composite;
mod condition;
mod control;
mod dead_code;
mod declaration_hints;
mod declarations;
mod deferred_declarations;
mod enum_guards;
mod exceptions;
mod expressions;
mod finally;
mod foreach_iterable;
mod function_calls;
mod generators;
mod if_ladder;
mod implicit_mixed;
mod in_array_guards;
mod inherited_parameters;
mod integer_range_contracts;
mod internal_methods;
mod interpolated_mixed;
mod intrinsics;
mod invalid_array_offset;
mod invocations;
mod late_static_new;
mod logical;
mod loop_mixed_values;
mod loop_writes;
mod loops;
mod match_expression;
mod member_checks;
mod member_expressions;
mod members;
mod missing_array_offset;
mod non_offset_receiver;
mod not_supported;
mod null_pairs;
mod null_test_flags;
mod nullsafe_guards;
mod operators;
mod overwritten_assignments;
mod phpdoc_fallthrough;
mod property_hooks;
mod property_writes;
mod receiver_types;
mod redundant_null_coalesce;
mod refine;
mod refine_types;
mod regex_captures;
mod reporting;
mod return_template_contracts;
mod short_circuit;
mod statements;
mod strict_mixed;
mod strict_rules;
mod string_replacements;
mod switch;
mod truthy_values;
mod type_docs;
mod unused_local_bindings;
mod unused_method_parameter;
mod value_contracts;
mod write;

pub const MAX_DEPTH: usize = 256;

#[derive(Clone)]
pub struct Context {
    pub names: Names,
    pub known: std::collections::BTreeSet<Symbol>,
    pub class: Option<String>,
    pub documentation_class: Option<String>,
    pub this_class: Option<String>,
    pub object: bool,
    pub unbound_this: Option<Vec<crate::index::Location>>,
    pub returns: ValueType,
    pub declared_return: bool,
    pub documented_return: Option<ValueType>,
    pub inferred_return: Option<ValueType>,
    pub generator: bool,
    pub by_reference: bool,
    pub function: String,
    pub scope_kind: ScopeKind,
    pub incomplete: bool,
}

pub struct Analyzer<'a> {
    checking_loop_mixed_values: bool,
    pub index: &'a mut Index,
    pub dependencies: &'a mut Dependencies,
    pub file: &'a ParsedFile,
    pub level: u8,
    pub options: &'a crate::config::Options,
    pub report_unused_traits: bool,
    pub strict_types: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub unverified_internal_ignores: std::collections::BTreeSet<usize>,
    pub context: Context,
    pub depth: usize,
    pub checked_declarations: std::collections::BTreeSet<usize>,
    pub exception_scopes: Vec<Vec<Scope>>,
    pub return_scopes: Vec<Vec<Scope>>,
    pub controls: Vec<control::ControlFrame>,
    pub statement_docs: std::collections::BTreeMap<usize, Span>,
    pub pending_callback_types: Vec<ValueType>,
    pub pending_binding: Option<(usize, closure_binding::ClosureBinding)>,
    pub last_condition: Option<Span>,
    pub discarded_expression: Option<Span>,
    pub trait_diagnostics: Option<Vec<(Span, Diagnostic)>>,
}

impl<'a> Analyzer<'a> {
    pub fn new(
        index: &'a mut Index,
        dependencies: &'a mut Dependencies,
        file: &'a ParsedFile,
        options: &'a crate::config::Options,
        report_unused_traits: bool,
    ) -> Self {
        let level = options.level;
        let read_docs = index.read_type_docs;
        Self {
            checking_loop_mixed_values: false,
            index,
            dependencies,
            file,
            level,
            options,
            report_unused_traits,
            strict_types: file.program.statements.iter().any(|statement| {
                matches!(&statement.kind, StatementKind::Declare { directives, .. }
                    if directives.iter().any(|directive| directive.name.name.eq_ignore_ascii_case("strict_types")
                        && matches!(directive.value.kind, ExpressionKind::Literal(Literal::Int(1)))))
            }),
            diagnostics: Vec::new(),
            unverified_internal_ignores: std::collections::BTreeSet::new(),
            depth: 0,
            checked_declarations: std::collections::BTreeSet::new(),
            exception_scopes: Vec::new(),
            return_scopes: Vec::new(),
            controls: Vec::new(),
            pending_binding: None,
            pending_callback_types: Vec::new(),
            last_condition: None,
            discarded_expression: None,
            trait_diagnostics: None,
            statement_docs: if read_docs {
                type_docs::statement_comments(&file.source)
            } else {
                Default::default()
            },
            context: Context {
                names: Names::for_block(String::new(), &file.program.statements),
                known: std::collections::BTreeSet::new(),
                class: None,
                documentation_class: None,
                this_class: None,
                object: false,
                unbound_this: None,
                returns: ValueType::Mixed,
                declared_return: false,
                documented_return: None,
                inferred_return: None,
                generator: false,
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
        crate::suppress::filter(
            self.file,
            &mut self.diagnostics,
            self.options.report_unmatched_ignores,
            &self.unverified_internal_ignores,
        );
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
            if name.text.eq_ignore_ascii_case("parent")
                && matches!(name.kind, NameKind::Unqualified)
                && let Some(owner) = &self.context.class
                && class
                    .as_ref()
                    .is_none_or(|class| self.index.class(class).is_none())
                && self
                    .index
                    .class(owner)
                    .is_some_and(|info| info.kind != ClassKind::Trait)
            {
                self.report(Finding::NoParent(owner.clone()), name.span);
                return ClassTarget::Unknown;
            }
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
        } else {
            match self.expression(expression, scope, false) {
                ValueType::Never => ClassTarget::Never,
                ValueType::ClassString(class) => {
                    class.class_name().map_or(ClassTarget::Unknown, |name| {
                        self.dependencies.resolve(self.index, name);
                        ClassTarget::Known(name.into())
                    })
                }
                _ => ClassTarget::Unknown,
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum Access {
    Checked,
    Inferred,
    Guarded,
    GuardedStatic,
    Static { has_object: bool, late_bound: bool },
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
        matches!(
            self,
            Self::Static {
                has_object: false,
                ..
            } | Self::GuardedStatic
                | Self::LateStatic { has_object: false }
        )
    }
    pub fn late_bound(self) -> bool {
        matches!(
            self,
            Self::Static {
                late_bound: true,
                ..
            } | Self::LateStatic { .. }
        )
    }
    pub fn dynamic_receiver(self) -> bool {
        matches!(self, Self::Checked | Self::LateStatic { .. })
    }
    pub fn reports_missing(self, level: u8) -> bool {
        match self {
            Self::Inferred => level >= 2,
            Self::Guarded | Self::GuardedStatic => false,
            _ => true,
        }
    }
}

#[derive(Clone, Copy)]
pub enum CallKind {
    Direct,
    Callable,
    InvalidReceiver,
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
