//! Print property declarations and their hooks in anonymous-class assertions.

use phera_ast::{
    ClassMemberKind, Expression, ExpressionKind, HookBody, Method, NewTarget, PropertyHook,
    Statement,
};
use phera_lexer::{Lexer, TokenKind};
use phera_source::Span;

pub(super) fn format(source: &[u8], argument: Span, condition: &Expression) -> Option<Vec<u8>> {
    let mut classes = Vec::new();
    let mut pending = vec![condition];
    while let Some(expression) = pending.pop() {
        match &expression.kind {
            ExpressionKind::Binary { left, right, .. } => {
                pending.push(right);
                pending.push(left);
            }
            ExpressionKind::New {
                class: NewTarget::Anonymous(class),
                ..
            } if class.members.iter().any(|member| match &member.kind {
                ClassMemberKind::Property { items, .. } => {
                    items.iter().any(|item| item.has_hook_list)
                }
                ClassMemberKind::Method(method) => method.parameters.iter().any(|parameter| {
                    parameter.has_hook_list
                        || parameter.is_promoted() && parameter.modifiers.is_final
                }),
                _ => false,
            }) =>
            {
                classes.push((expression.span, class));
            }
            _ => {}
        }
    }
    if classes.is_empty() {
        return None;
    }
    let mut output = Vec::new();
    let mut previous = argument.start;
    for (span, class) in classes {
        output.extend_from_slice(source.get(previous..span.start)?);
        let first = class.members.first()?.span.start;
        opening(&mut output, 0, source.get(span.start..first)?)?;
        for member in &class.members {
            match &member.kind {
                ClassMemberKind::Property { items, .. }
                    if items.iter().any(|item| item.has_hook_list) =>
                {
                    let [property] = items.as_slice() else {
                        return None;
                    };
                    indent(&mut output, 1);
                    hook_list(&mut output, source, member.span, &property.hooks, 1)?;
                    output.push(b'\n');
                }
                ClassMemberKind::Method(method) if method.body.is_some() => {
                    method_block(&mut output, source, member.span, method)?;
                    output.push(b'\n');
                }
                _ => line(&mut output, 1, slice(source, member.span)?),
            }
        }
        output.push(b'}');
        previous = span.end;
    }
    output.extend_from_slice(source.get(previous..argument.end)?);
    Some(output)
}

fn slice(source: &[u8], span: Span) -> Option<&[u8]> {
    source.get(span.start..span.end)
}

fn opening(output: &mut Vec<u8>, depth: usize, text: &[u8]) -> Option<()> {
    indent(output, depth);
    output.extend_from_slice(before_opening(text)?);
    output.extend_from_slice(b" {\n");
    Some(())
}

fn before_opening(text: &[u8]) -> Option<&[u8]> {
    let mut tokens = Lexer::new_php(std::str::from_utf8(text).ok()?);
    let mut brace = None;
    loop {
        let token = tokens.next_token().ok()?;
        match token.kind {
            TokenKind::Eof => break,
            TokenKind::LeftBrace => brace = Some(token.span.start),
            _ => {}
        }
    }
    Some(text.get(..brace?)?.trim_ascii())
}

fn hook_list(
    output: &mut Vec<u8>,
    source: &[u8],
    span: Span,
    hooks: &[PropertyHook],
    depth: usize,
) -> Option<()> {
    let first = hooks.first().map_or(span.end, |hook| hook.span.start);
    output.extend_from_slice(before_opening(source.get(span.start..first)?)?);
    output.extend_from_slice(b" {\n");
    for hook in hooks {
        match &hook.body {
            HookBody::None => line(output, depth + 1, slice(source, hook.span)?),
            HookBody::Expression(expression) => {
                let before = source.get(hook.span.start..expression.span.start)?;
                indent(output, depth + 1);
                output.extend_from_slice(before.trim_ascii().strip_suffix(b"=>")?.trim_ascii_end());
                output.extend_from_slice(b" => ");
                output.extend_from_slice(slice(source, expression.span)?.trim_ascii());
                output.extend_from_slice(b";\n");
            }
            HookBody::Block(statements) => block(output, source, hook.span, statements, depth + 1)?,
        }
    }
    indent(output, depth);
    output.push(b'}');
    Some(())
}

fn method_block(output: &mut Vec<u8>, source: &[u8], span: Span, method: &Method) -> Option<()> {
    let statements = method.body.as_deref()?;
    if !method
        .parameters
        .iter()
        .any(|parameter| parameter.has_hook_list)
    {
        return block(output, source, span, statements, 1);
    }
    let first = statements.first().map_or(span.end, |stmt| stmt.span.start);
    indent(output, 1);
    output.extend_from_slice(source.get(span.start..method.name.span.end)?.trim_ascii());
    output.push(b'(');
    for (index, parameter) in method.parameters.iter().enumerate() {
        if index != 0 {
            output.extend_from_slice(b", ");
        }
        if parameter.has_hook_list {
            hook_list(output, source, parameter.span, &parameter.hooks, 1)?;
        } else {
            output.extend_from_slice(slice(source, parameter.span)?.trim_ascii());
        }
    }
    let end = method.parameters.last()?.span.end;
    let tail = source.get(end..first)?.trim_ascii_start();
    output.push(b')');
    output.extend_from_slice(before_opening(tail.strip_prefix(b")")?)?);
    output.extend_from_slice(b" {\n");
    for statement in statements {
        line(output, 2, slice(source, statement.span)?);
    }
    line(output, 1, b"}");
    Some(())
}

fn block(
    output: &mut Vec<u8>,
    source: &[u8],
    span: Span,
    statements: &[Statement],
    depth: usize,
) -> Option<()> {
    let first = statements.first().map_or(span.end, |stmt| stmt.span.start);
    opening(output, depth, source.get(span.start..first)?)?;
    for statement in statements {
        line(output, depth + 1, slice(source, statement.span)?);
    }
    line(output, depth, b"}");
    Some(())
}

fn line(output: &mut Vec<u8>, depth: usize, text: &[u8]) {
    indent(output, depth);
    output.extend_from_slice(text.trim_ascii());
    output.push(b'\n');
}

fn indent(output: &mut Vec<u8>, depth: usize) {
    for _ in 0..depth {
        output.extend_from_slice(b"    ");
    }
}
