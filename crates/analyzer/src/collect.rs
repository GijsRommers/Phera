//! Declaration collection: `namespace N; function f(int $x): string { ... }`.
//! Imports belong to a namespace block, including declarations inside conditions.

pub use crate::collect_class::class_info;
use crate::{index::*, names::Names};
use phera_ast::*;
use phera_source::LineTable;

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
            for block in collect_expression_declarations(index, statement, &names, &location) {
                pending.push((block, names.clone()));
            }
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
                        let name = names.declaration(&name.name).to_ascii_lowercase();
                        if !index.builtin_classes.contains(&name) {
                            index.classes.insert(name, info);
                        }
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
                    let signature =
                        function_signature(function, &names, location(statement.span.start));
                    let name = names.declaration(&function.name.name).to_ascii_lowercase();
                    if !index.builtin_functions.contains(&name) {
                        index.functions.insert(name, signature);
                    }
                    pending.push((&function.body, names.clone()));
                }
                StatementKind::Const { items, .. } => {
                    collect_constants(index, items, &names, &location)
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

pub(crate) fn uses_argument_introspection(body: &[Statement]) -> bool {
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
                            name.text.to_ascii_lowercase().as_str(),
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

fn collect_expression_declarations<'a>(
    index: &mut Index,
    statement: &'a Statement,
    names: &Names,
    location: &impl Fn(usize) -> Location,
) -> Vec<&'a [Statement]> {
    let mut blocks = Vec::new();
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
            for member in &class.members {
                if let ClassMemberKind::Method(method) = &member.kind
                    && let Some(body) = &method.body
                {
                    blocks.push(body.as_slice());
                }
            }
            let mut info = class_info(class, names, location);
            info.name = anonymous_name(&location(class.span.start).file, class.span.start);
            index.classes.insert(info.name.to_ascii_lowercase(), info);
        }
        match &expression.kind {
            ExpressionKind::Closure(closure) => blocks.push(closure.body.as_slice()),
            ExpressionKind::ArrowFunction(arrow) => pending.push(&arrow.body),
            _ => {}
        }
        pending.extend(crate::walk::children(expression));
    }
    blocks
}

pub fn anonymous_name(file: &str, offset: usize) -> String {
    format!("@anonymous:{file}:{offset}")
}

fn function_signature(
    function: &FunctionDeclaration,
    names: &Names,
    location: Location,
) -> Signature {
    let mut signature = Signature::new(
        &function.parameters,
        function.return_type.as_ref(),
        names,
        location,
    );
    signature.accepts_extra = uses_argument_introspection(&function.body);
    signature.global_effects = crate::global_effects::of(&function.body);
    signature
}

fn collect_constants(
    index: &mut Index,
    items: &[ConstantDeclaration],
    names: &Names,
    location: &impl Fn(usize) -> Location,
) {
    for item in items {
        let location = location(item.span.start);
        let ty = crate::constant_types::infer(&item.value, names, location.clone(), index);
        index.constants.insert(
            names.declaration(&item.name.name),
            ConstantInfo {
                ty,
                modifiers: Modifiers::default(),
                location,
            },
        );
    }
}
