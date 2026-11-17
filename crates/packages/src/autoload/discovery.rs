//! Discover named PHP types throughout the AST, including methods and closures.
//!
//! `class Outer { function f() { class Inner {} } }` maps both Outer and Inner,
//! matching Composer's class-map generator. Expression chains use a heap worklist.

use super::expressions::expression;
use phera_ast::{ClassDeclaration, ClassMemberKind, HookBody, Statement, StatementKind, Variable};

pub(super) fn statements(block: &[Statement], namespace: &str, names: &mut Vec<String>) {
    for statement in block {
        match &statement.kind {
            StatementKind::Namespace {
                name,
                statements: body,
                ..
            } => {
                statements(body, name.as_ref().map_or("", |name| &name.text), names);
            }
            StatementKind::Class(class) => declaration(class, namespace, names),
            StatementKind::Function(function) => statements(&function.body, namespace, names),
            StatementKind::Block(body) => statements(body, namespace, names),
            StatementKind::If {
                condition,
                then,
                otherwise,
            } => {
                expression(condition, namespace, names);
                statements(then, namespace, names);
                if let Some(body) = otherwise {
                    statements(body, namespace, names);
                }
            }
            StatementKind::While { condition, body }
            | StatementKind::DoWhile { condition, body } => {
                expression(condition, namespace, names);
                statements(body, namespace, names);
            }
            StatementKind::For {
                init,
                condition,
                step,
                body,
            } => {
                for value in init.iter().chain(condition).chain(step) {
                    expression(value, namespace, names);
                }
                statements(body, namespace, names);
            }
            StatementKind::Foreach {
                subject,
                key,
                value,
                body,
                ..
            } => {
                expression(subject, namespace, names);
                if let Some(key) = key {
                    expression(key, namespace, names);
                }
                expression(value, namespace, names);
                statements(body, namespace, names);
            }
            StatementKind::Try {
                body,
                catches,
                finally,
            } => {
                statements(body, namespace, names);
                for catch in catches {
                    statements(&catch.body, namespace, names);
                }
                if let Some(body) = finally {
                    statements(body, namespace, names);
                }
            }
            StatementKind::Switch { subject, cases } => {
                expression(subject, namespace, names);
                for case in cases {
                    if let Some(value) = &case.condition {
                        expression(value, namespace, names);
                    }
                    statements(&case.body, namespace, names);
                }
            }
            StatementKind::Declare {
                body: Some(body), ..
            } => statements(body, namespace, names),
            StatementKind::Expression(value) | StatementKind::Return(Some(value)) => {
                expression(value, namespace, names);
            }
            StatementKind::Echo(values) | StatementKind::Unset(values) => {
                for value in values {
                    expression(value, namespace, names);
                }
            }
            StatementKind::Static(variables) => {
                for variable in variables {
                    if let Some(value) = &variable.default {
                        expression(value, namespace, names);
                    }
                }
            }
            StatementKind::Global(variables) => {
                for variable in variables {
                    if let Variable::Dynamic(value) = variable {
                        expression(value, namespace, names);
                    }
                }
            }
            _ => {}
        }
    }
}

pub(super) fn declaration(class: &ClassDeclaration, namespace: &str, names: &mut Vec<String>) {
    if let Some(name) = &class.name {
        names.push(if namespace.is_empty() {
            name.name.clone()
        } else {
            format!("{namespace}\\{}", name.name)
        });
    }
    for member in &class.members {
        match &member.kind {
            ClassMemberKind::Method(method) => {
                if let Some(body) = &method.body {
                    statements(body, namespace, names);
                }
            }
            ClassMemberKind::Property { items, .. } => {
                for property in items {
                    for hook in &property.hooks {
                        match &hook.body {
                            HookBody::Block(body) => statements(body, namespace, names),
                            HookBody::Expression(value) => expression(value, namespace, names),
                            HookBody::None => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn discovers_method_closure_and_anonymous_method_declarations() {
        // Observed with Composer ClassMapGenerator 1.9.0 under PHP 8.5.12.
        let source = "<?php namespace Acme; class OuterClass {\n\
            function factory() { class MethodNested {} } }\n\
            $factory = static function () { class ClosureNested {} };\n\
            $anonymous = new class { function factory() { class AnonymousMethodNested {} } };";
        let program = phera_parser::parse(source).unwrap();
        let mut names = Vec::new();
        super::statements(&program.statements, "", &mut names);
        assert_eq!(
            names,
            [
                "Acme\\OuterClass",
                "Acme\\MethodNested",
                "Acme\\ClosureNested",
                "Acme\\AnonymousMethodNested"
            ]
        );
    }
}
