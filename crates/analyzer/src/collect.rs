//! Declaration collection: `namespace N; function f(int $x): string { ... }`.
//! Imports belong to a namespace block, including declarations inside conditions.

use crate::{index::*, names::Names, types::ValueType};
use phera_ast::*;
use phera_source::LineTable;
use std::collections::BTreeMap;

pub fn collect(index: &mut Index, file: &str, source: &str, program: &Program) {
    let lines = LineTable::new(source);
    let names = Names::for_block(String::new(), &program.statements);
    let mut pending = vec![(&program.statements[..], names)];
    while let Some((statements, names)) = pending.pop() {
        for statement in statements {
            let location = |offset| Location {
                file: file.into(),
                line: lines.line(offset),
            };
            collect_expression_declarations(index, statement, &names, &location);
            match &statement.kind {
                StatementKind::Namespace {
                    name, statements, ..
                } => {
                    let namespace = name.as_ref().map_or(String::new(), |n| n.text.clone());
                    pending.push((statements, Names::for_block(namespace, statements)));
                }
                StatementKind::Class(class) => {
                    if let Some(name) = &class.name {
                        let info = class_info(class, &names, &location);
                        index
                            .classes
                            .insert(names.declaration(&name.name).to_lowercase(), info);
                    }
                    for member in &class.members {
                        if let ClassMemberKind::Method(method) = &member.kind
                            && let Some(body) = &method.body
                        {
                            pending.push((body, names.clone()));
                        }
                    }
                }
                StatementKind::Function(function) => {
                    let mut signature = Signature::new(
                        &function.parameters,
                        function.return_type.as_ref(),
                        &names,
                        location(statement.span.start),
                    );
                    signature.accepts_extra = uses_argument_introspection(&function.body);
                    let name = names.declaration(&function.name.name).to_lowercase();
                    if !index.builtin_functions.contains(&name) {
                        index.functions.insert(name, signature);
                    }
                    pending.push((&function.body, names.clone()));
                }
                StatementKind::Const { items, .. } => {
                    for item in items {
                        index.constants.insert(
                            names.declaration(&item.name.name),
                            ConstantInfo::untyped(location(item.span.start)),
                        );
                    }
                }
                _ => {
                    for block in child_blocks(statement) {
                        pending.push((block, names.clone()));
                    }
                }
            }
        }
    }
}

pub fn class_info(
    class: &ClassDeclaration,
    names: &Names,
    location: &impl Fn(usize) -> Location,
) -> ClassInfo {
    let resolve = |n: &Name| names.resolve(n, UseKind::Class)[0].clone();
    let mut info = ClassInfo {
        name: names.declaration(class.name.as_ref().map_or("@anonymous", |n| &n.name)),
        kind: class.kind,
        modifiers: class.modifiers,
        parents: class.extends.iter().map(resolve).collect(),
        interfaces: class.implements.iter().map(resolve).collect(),
        traits: Vec::new(),
        methods: BTreeMap::new(),
        properties: BTreeMap::new(),
        constants: BTreeMap::new(),
        location: location(class.span.start),
        open_properties: false,
    };
    for member in &class.members {
        match &member.kind {
            ClassMemberKind::Method(method) => {
                let mut signature = Signature::new(
                    &method.parameters,
                    method.return_type.as_ref(),
                    names,
                    location(member.span.start),
                );
                signature.modifiers = method.modifiers;
                signature.abstract_body =
                    method.body.is_none() || class.kind == ClassKind::Interface;
                signature.accepts_extra = method
                    .body
                    .as_ref()
                    .is_some_and(|body| uses_argument_introspection(body));
                info.methods
                    .insert(method.name.name.to_lowercase(), signature);
                for p in &method.parameters {
                    if p.modifiers.visibility.is_some() {
                        info.properties.insert(
                            p.name.clone(),
                            PropertyInfo {
                                ty: ValueType::declared(p.ty.as_ref(), names),
                                modifiers: p.modifiers,
                                location: location(p.span.start),
                            },
                        );
                    }
                }
            }
            ClassMemberKind::Property {
                modifiers,
                ty,
                items,
                ..
            } => {
                for p in items {
                    info.properties.insert(
                        p.name.clone(),
                        PropertyInfo {
                            ty: ValueType::declared(ty.as_ref(), names),
                            modifiers: *modifiers,
                            location: location(p.span.start),
                        },
                    );
                }
            }
            ClassMemberKind::Constant {
                modifiers,
                ty,
                items,
                ..
            } => {
                for c in items {
                    info.constants.insert(
                        c.name.name.clone(),
                        ConstantInfo {
                            ty: ValueType::declared(ty.as_ref(), names),
                            modifiers: *modifiers,
                            location: location(c.span.start),
                        },
                    );
                }
            }
            ClassMemberKind::EnumCase { name, .. } => {
                info.constants.insert(
                    name.name.clone(),
                    ConstantInfo {
                        ty: ValueType::Class(info.name.clone()),
                        modifiers: Modifiers::default(),
                        location: location(name.span.start),
                    },
                );
            }
            ClassMemberKind::TraitUse { traits, .. } => {
                info.traits.extend(traits.iter().map(resolve));
            }
        }
    }
    crate::enums::add_members(&mut info, class, names);
    info.open_properties = class
        .attributes
        .iter()
        .any(|a| a.name.text.eq_ignore_ascii_case("AllowDynamicProperties"));
    info
}

pub fn child_blocks(statement: &Statement) -> Vec<&[Statement]> {
    match &statement.kind {
        StatementKind::Block(body)
        | StatementKind::While { body, .. }
        | StatementKind::DoWhile { body, .. }
        | StatementKind::For { body, .. }
        | StatementKind::Foreach { body, .. } => vec![body],
        StatementKind::If {
            then, otherwise, ..
        } => {
            let mut blocks = vec![then.as_slice()];
            blocks.extend(otherwise.as_deref());
            blocks
        }
        StatementKind::Switch { cases, .. } => cases.iter().map(|c| c.body.as_slice()).collect(),
        StatementKind::Try {
            body,
            catches,
            finally,
        } => {
            let mut blocks = vec![body.as_slice()];
            blocks.extend(catches.iter().map(|c| c.body.as_slice()));
            blocks.extend(finally.as_deref());
            blocks
        }
        StatementKind::Declare { body, .. } => body.as_deref().into_iter().collect(),
        _ => Vec::new(),
    }
}

fn uses_argument_introspection(body: &[Statement]) -> bool {
    // The complete expression walker is iterative, including long postfix chains.
    let mut blocks = vec![body];
    while let Some(block) = blocks.pop() {
        for statement in block {
            blocks.extend(child_blocks(statement));
            for expression in crate::walk::statement_expressions(statement) {
                let mut expressions = vec![expression];
                while let Some(expr) = expressions.pop() {
                    if let ExpressionKind::Call { callee, .. } = &expr.kind
                        && let ExpressionKind::Name(name) = &callee.kind
                        && matches!(
                            name.text.to_lowercase().as_str(),
                            "func_get_args" | "func_get_arg" | "func_num_args"
                        )
                    {
                        return true;
                    }
                    expressions.extend(crate::walk::children(expr));
                }
            }
        }
    }
    false
}

fn collect_expression_declarations(
    index: &mut Index,
    statement: &Statement,
    names: &Names,
    location: &impl Fn(usize) -> Location,
) {
    let mut pending = crate::walk::statement_expressions(statement);
    while let Some(expression) = pending.pop() {
        if let ExpressionKind::Call {
            callee,
            arguments: Arguments::List(arguments),
        } = &expression.kind
            && let ExpressionKind::Name(name) = &callee.kind
            && name.text.eq_ignore_ascii_case("define")
            && let Some(argument) = arguments.first()
            && let ExpressionKind::Literal(Literal::String(name)) = &argument.value.kind
            && let Ok(name) = std::str::from_utf8(name)
        {
            index.constants.insert(
                name.trim_start_matches('\\').into(),
                ConstantInfo::untyped(location(expression.span.start)),
            );
        }
        if let ExpressionKind::New {
            class: NewTarget::Anonymous(class),
            ..
        } = &expression.kind
        {
            let mut info = class_info(class, names, location);
            info.name = anonymous_name(&location(class.span.start).file, class.span.start);
            index.classes.insert(info.name.to_lowercase(), info);
        }
        pending.extend(crate::walk::children(expression));
    }
}

pub fn anonymous_name(file: &str, offset: usize) -> String {
    format!("@anonymous:{file}:{offset}")
}
