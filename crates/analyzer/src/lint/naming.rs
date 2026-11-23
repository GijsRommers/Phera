//! Naming conventions distinguish `class UserReport` from `function userReport()`.

use super::{
    Context,
    options::VariableStyle,
    rule::{Check, DefaultStatus, Group, Rule, Visit},
    traversal::Node,
};
use phera_ast::{
    ClassMemberKind as Member, ExpressionKind as Expr, StatementKind as Stmt, Variable,
};
use phera_source::Span;
use std::path::Path;

pub(super) const RULES: &[Rule] = &[
    naming(
        "class-name",
        "Use PascalCase names for classes, interfaces, traits and enums.",
        "class user_report {}",
        "class UserReport {}",
        Visit::Class,
        class_name,
    ),
    naming(
        "method-name",
        "Use camelCase names for methods.",
        "class C { function read_report() {} }",
        "class C { function readReport() {} }",
        Visit::Method,
        method_name,
    ),
    naming(
        "property-name",
        "Use camelCase names for properties.",
        "class C { public $Report_Name; }",
        "class C { public $reportName; }",
        Visit::Class,
        property_name,
    ),
    naming(
        "constant-name",
        "Use uppercase underscore-separated names for constants.",
        "const ReportName = 1;",
        "const REPORT_NAME = 1;",
        Visit::Statement,
        constant_name,
    ),
    naming(
        "variable-name",
        "Follow the configured camel or snake case for variable names.",
        "$report_name = 1;",
        "$reportName = 1;",
        Visit::Expression,
        variable_name,
    ),
    naming(
        "one-class-per-file",
        "Declare one named class, interface, trait or enum per file.",
        "class First {} class Second {}",
        "class First {}",
        Visit::File,
        one_class,
    ),
    naming(
        "class-file-name",
        "Name each PHP file after its class or other class-like declaration.",
        "class OtherName {}",
        "class good {}",
        Visit::Class,
        file_name,
    ),
    naming(
        "psr4-namespace",
        "Match the namespace to a Composer PSR-4 path.",
        "namespace Wrong; class Report {}",
        "namespace Vendor; class Report {}",
        Visit::Class,
        psr4,
    ),
];

const fn naming(
    name: &'static str,
    description: &'static str,
    bad: &'static str,
    good: &'static str,
    visit: Visit,
    check: Check,
) -> Rule {
    Rule {
        name,
        description,
        bad,
        good,
        visit,
        check: Some(check),
        fix: "",
        default: DefaultStatus::OptIn,
        group: Group::Naming,
    }
}

fn class_name(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Class(class) = node
        && let Some(name) = &class.name
        && !pascal(&name.name)
    {
        context.report(
            "class-name",
            name.span,
            "Write this class-like name in PascalCase.",
        );
    }
}

fn method_name(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Method(method) = node
        && !method.name.name.starts_with("__")
        && !camel(&method.name.name)
    {
        context.report(
            "method-name",
            method.name.span,
            "Write this method name in camelCase.",
        );
    }
}

fn property_name(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Class(class) = node else {
        return;
    };
    for member in &class.members {
        match &member.kind {
            Member::Property { items, .. } => {
                for item in items {
                    check_property(context, &item.name, item.span);
                }
            }
            Member::Method(method) => {
                for parameter in method
                    .parameters
                    .iter()
                    .filter(|parameter| parameter.is_promoted())
                {
                    check_property(context, &parameter.name, parameter.span);
                }
            }
            _ => {}
        }
    }
}

fn check_property(context: &mut Context<'_>, name: &str, span: Span) {
    if !camel(name) {
        context.report(
            "property-name",
            span,
            "Write this property name in camelCase.",
        );
    }
}

fn constant_name(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Statement(statement) = node else {
        return;
    };
    let mut check = |constant: &phera_ast::ConstantDeclaration| {
        if !constant
            .name
            .name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_uppercase)
            || !constant
                .name
                .name
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            context.report(
                "constant-name",
                constant.name.span,
                "Write this constant name in uppercase underscore-separated form.",
            );
        }
    };
    match &statement.kind {
        Stmt::Const { items, .. } => items.iter().for_each(&mut check),
        Stmt::Class(class) => {
            for member in &class.members {
                if let Member::Constant { items, .. } = &member.kind {
                    items.iter().for_each(&mut check);
                }
            }
        }
        _ => {}
    }
}

fn variable_name(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expression, _) = node else {
        return;
    };
    let Expr::Variable(Variable::Named(name)) = &expression.kind else {
        return;
    };
    if name == "GLOBALS" || name.starts_with('_') {
        return;
    }
    let (valid, style) = match context.options.variables {
        VariableStyle::Camel => (camel(name), "camelCase"),
        VariableStyle::Snake => (
            name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "snake_case",
        ),
    };
    if !valid {
        context.report(
            "variable-name",
            expression.span,
            format!("Write this variable name in {style}."),
        );
    }
}

fn one_class(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Block(block) = node else {
        return;
    };
    let mut pending = vec![block.iter()];
    let mut found = false;
    while let Some(statements) = pending.last_mut() {
        match statements.next().map(|statement| &statement.kind) {
            Some(Stmt::Namespace { statements, .. }) => pending.push(statements.iter()),
            Some(Stmt::Class(class)) => {
                if let Some(name) = &class.name
                    && std::mem::replace(&mut found, true)
                {
                    context.report(
                        "one-class-per-file",
                        name.span,
                        "Declare this class-like symbol in its own file.",
                    );
                }
            }
            Some(_) => {}
            None => {
                pending.pop();
            }
        }
    }
}

fn file_name(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Class(class) = node
        && let Some(name) = &class.name
        && !Path::new(context.file)
            .file_stem()
            .is_some_and(|file| file == name.name.as_str())
    {
        context.report(
            "class-file-name",
            name.span,
            format!("Name this file '{}.php'.", name.name),
        );
    }
}

fn psr4(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Class(class) = node
        && let Some(name) = &class.name
        && context.options.manifest.as_ref().and_then(|manifest| {
            manifest.namespace_matches(Path::new(context.file), &context.names.namespace)
        }) == Some(false)
    {
        context.report(
            "psr4-namespace",
            name.span,
            "This namespace does not match the file's Composer PSR-4 path.",
        );
    }
}

fn pascal(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && name.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn camel(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && name.bytes().all(|b| b.is_ascii_alphanumeric())
}
