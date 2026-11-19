//! Preserve PHP token meaning while normalizing forms such as `TRUE` to `true`.

use super::style;
use crate::lint::{
    Context,
    fix::{Edit, Fix},
    rule::{Rule, Visit},
    source::is_comment,
    traversal::Node,
};
use phera_ast::TypeKind;
use phera_lexer::{Token, TokenKind as Kind};
use phera_source::Span;

pub(super) const RULES: &[Rule] = &[
    style(
        "keyword-case",
        "Write PHP keywords and built-in literal names in lowercase.",
        "IF (TRUE) { echo 1; }",
        "if (true) { echo 1; }",
        "Safe token fix.",
        Visit::File,
        keyword_case,
    ),
    style(
        "type-case",
        "Write native PHP types in lowercase.",
        "function f(INT $n): VOID {}",
        "function f(int $n): void {}",
        "Safe token fix.",
        Visit::Type,
        type_case,
    ),
    style(
        "elseif",
        "Write elseif as one keyword.",
        "if ($a) {} else if ($b) {}",
        "if ($a) {} elseif ($b) {}",
        "Safe fix on a single line with whitespace between the keywords.",
        Visit::File,
        elseif,
    ),
    style(
        "no-closing-tag",
        "Omit the closing PHP tag from PHP-only files.",
        "<?php\necho 1;\n?>",
        "<?php\necho 1;",
        "Safe when the preceding statement has an explicit terminator.",
        Visit::File,
        closing_tag,
    ),
    style(
        "final-newline",
        "End a PHP-only file with one line feed and no blank lines.",
        "<?php\necho 1;\n\n",
        "<?php\necho 1;",
        "Safe fix in trailing PHP trivia.",
        Visit::File,
        final_newline,
    ),
    style(
        "trailing-whitespace",
        "Remove horizontal whitespace before line endings in PHP trivia.",
        "<?php\necho 1;  \necho 2;",
        "<?php\necho 1;\necho 2;",
        "Safe token fix outside strings and comments.",
        Visit::File,
        trailing_whitespace,
    ),
    style(
        "short-cast",
        "Use the short spellings of native casts.",
        "$n = (integer) $value;",
        "$n = (int) $value;",
        "Safe token fix for the cast's spelling. It removes its deprecation notice.",
        Visit::File,
        casts,
    ),
];

fn keyword_case(context: &mut Context<'_>, _: Node<'_>) {
    let mut previous = None;
    let mut tokens = context
        .source
        .tokens()
        .iter()
        .filter(|token| !is_comment(token) && token.kind != Kind::Whitespace)
        .peekable();
    let mut edits = Vec::new();
    while let Some(token) = tokens.next() {
        let bytes = context.source.slice(token.span);
        let original = previous.replace(token.kind);
        if !is_keyword(token.kind)
            || matches!(
                original,
                Some(
                    Kind::Arrow
                        | Kind::NullsafeArrow
                        | Kind::DoubleColon
                        | Kind::Function
                        | Kind::Ampersand
                        | Kind::Const
                )
            )
            || tokens.peek().is_some_and(|next| next.kind == Kind::Colon)
            || !bytes.iter().any(u8::is_ascii_uppercase)
        {
            continue;
        }
        edits.push((token.span, bytes.to_ascii_lowercase()));
    }
    report_edits(
        context,
        "keyword-case",
        "Write this keyword in lowercase.",
        edits,
    );
}

fn is_keyword(kind: Kind) -> bool {
    use Kind::*;
    matches!(
        kind,
        Abstract
            | And
            | Array
            | As
            | Break
            | Case
            | Catch
            | Class
            | Clone
            | Const
            | Continue
            | Declare
            | Default
            | Do
            | Echo
            | Else
            | ElseIf
            | Empty
            | EndDeclare
            | EndFor
            | EndForeach
            | EndIf
            | EndSwitch
            | EndWhile
            | Eval
            | Extends
            | False
            | Final
            | Finally
            | Fn
            | For
            | Foreach
            | Function
            | Global
            | Goto
            | If
            | Implements
            | Include
            | IncludeOnce
            | InstanceOf
            | InsteadOf
            | Interface
            | Isset
            | List
            | Match
            | Namespace
            | New
            | Null
            | Or
            | Print
            | Private
            | Protected
            | Public
            | Readonly
            | Require
            | RequireOnce
            | Return
            | Static
            | Switch
            | Throw
            | Trait
            | True
            | Try
            | Unset
            | Use
            | Var
            | While
            | Xor
            | Yield
    )
}

fn type_case(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Type(ty) = node else {
        return;
    };
    let TypeKind::Named(name) = &ty.kind else {
        return;
    };
    let lower = name.text.to_ascii_lowercase();
    if name.text != lower
        && matches!(
            lower.as_str(),
            "array"
                | "bool"
                | "callable"
                | "false"
                | "float"
                | "int"
                | "iterable"
                | "mixed"
                | "never"
                | "null"
                | "object"
                | "parent"
                | "self"
                | "static"
                | "string"
                | "true"
                | "void"
        )
    {
        context.with_fix(
            "type-case",
            name.span,
            "Write this native type in lowercase.",
            Fix::safe(vec![Edit::replace(name.span, lower.into_bytes())]),
        );
    }
}

fn elseif(context: &mut Context<'_>, _: Node<'_>) {
    let edits = context
        .source
        .tokens()
        .iter()
        .filter(|token| token.kind != Kind::Whitespace)
        .zip(
            context
                .source
                .tokens()
                .iter()
                .filter(|token| token.kind != Kind::Whitespace)
                .skip(1),
        )
        .filter(|(a, b)| {
            a.kind == Kind::Else
                && b.kind == Kind::If
                && context.lines.line(a.span.start) == context.lines.line(b.span.start)
        })
        .map(|(a, b)| (Span::new(a.span.start, b.span.end), b"elseif".to_vec()))
        .collect();
    report_edits(context, "elseif", "Write elseif as one keyword.", edits);
}

fn pure_php(tokens: &[Token]) -> bool {
    tokens
        .first()
        .is_some_and(|token| token.kind == Kind::OpenTag)
        && tokens.iter().all(|token| token.kind != Kind::InlineHtml)
}

fn closing_tag(context: &mut Context<'_>, _: Node<'_>) {
    let tokens = context.source.tokens();
    if !pure_php(tokens) {
        return;
    }
    if let Some(tag) = tokens.last().filter(|token| token.kind == Kind::CloseTag) {
        let span = Span::new(tag.span.start, tag.span.start + 2);
        let previous = tokens
            .iter()
            .rev()
            .skip(1)
            .find(|token| !is_comment(token) && token.kind != Kind::Whitespace);
        if previous.is_some_and(|token| {
            matches!(
                token.kind,
                Kind::Semicolon | Kind::RightBrace | Kind::OpenTag
            )
        }) {
            context.with_fix(
                "no-closing-tag",
                span,
                "Omit the closing PHP tag.",
                Fix::safe(vec![Edit::remove(span)]),
            );
        } else {
            context.report(
                "no-closing-tag",
                span,
                "Omit the closing PHP tag after adding the statement terminator.",
            );
        }
    }
}

fn final_newline(context: &mut Context<'_>, _: Node<'_>) {
    let bytes = context.source.bytes;
    let tokens = context.source.tokens();
    if !pure_php(tokens)
        || tokens
            .last()
            .is_some_and(|token| token.kind == Kind::CloseTag)
    {
        return;
    }
    let end = bytes
        .iter()
        .rposition(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        .map_or(0, |offset| offset + 1);
    if bytes.get(end..) == Some(b"\n") {
        return;
    }
    let span = Span::new(end, bytes.len());
    if tokens
        .last()
        .is_some_and(|last| last.kind == Kind::Whitespace || last.span.end <= end)
    {
        context.with_fix(
            "final-newline",
            span,
            "End the file with one line feed.",
            Fix::safe(vec![Edit::replace(span, b"\n".to_vec())]),
        );
    }
}

fn trailing_whitespace(context: &mut Context<'_>, _: Node<'_>) {
    let mut edits = Vec::new();
    for token in context
        .source
        .tokens()
        .iter()
        .filter(|token| token.kind == Kind::Whitespace)
    {
        let mut horizontal_start = None;
        for (offset, byte) in context.source.slice(token.span).iter().copied().enumerate() {
            match byte {
                b' ' | b'\t' => {
                    horizontal_start.get_or_insert(token.span.start + offset);
                }
                b'\n' | b'\r' => {
                    if let Some(start) = horizontal_start.take() {
                        edits.push((Span::new(start, token.span.start + offset), Vec::new()));
                    }
                }
                _ => {
                    horizontal_start = None;
                }
            }
        }
    }
    report_edits(
        context,
        "trailing-whitespace",
        "Remove trailing whitespace.",
        edits,
    );
}

fn casts(context: &mut Context<'_>, _: Node<'_>) {
    let edits = context
        .source
        .tokens()
        .iter()
        .filter(|token| {
            matches!(
                token.kind,
                Kind::IntCast | Kind::BoolCast | Kind::FloatCast | Kind::StringCast
            )
        })
        .filter_map(|token| {
            let name = context
                .source
                .slice(token.span)
                .iter()
                .copied()
                .filter(|byte| !byte.is_ascii_whitespace() && !matches!(byte, b'(' | b')'))
                .map(|b| b.to_ascii_lowercase())
                .collect::<Vec<_>>();
            let replacement = match name.as_slice() {
                b"integer" => b"(int)".as_slice(),
                b"boolean" => b"(bool)",
                b"double" => b"(float)",
                b"binary" => b"(string)",
                _ => return None,
            };
            (!context.source.slice(token.span).contains(&b'\n')
                && !context.source.slice(token.span).contains(&b'\r'))
            .then(|| (token.span, replacement.to_vec()))
        })
        .collect();
    report_edits(
        context,
        "short-cast",
        "Use the short spelling of this cast.",
        edits,
    );
}

fn report_edits(
    context: &mut Context<'_>,
    rule: &'static str,
    message: &'static str,
    edits: Vec<(Span, Vec<u8>)>,
) {
    for (span, replacement) in edits {
        context.with_fix(
            rule,
            span,
            message,
            Fix::safe(vec![Edit::replace(span, replacement)]),
        );
    }
}
