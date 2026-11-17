//! Adoption comments: `missing(); // @phpstan-ignore function.notFound`.
//! Comments are read only from lexer gaps, so a string containing an ignore is harmless.

use crate::{diagnostic::Diagnostic, project::ParsedFile};
use phera_lexer::{Lexer, TokenKind};

pub fn suppressed(file: &ParsedFile, diagnostic: &Diagnostic) -> bool {
    let mut lexer = Lexer::new(&file.source);
    let mut end = 0;
    while let Ok(token) = lexer.next_token() {
        let gap = &file.source[end..token.span.start.max(end)];
        for (offset, line) in gap.split_inclusive('\n').scan(end, |offset, text| {
            let start = *offset;
            *offset += text.len();
            Some((start, text))
        }) {
            if !line.contains('@') {
                continue;
            }
            let comment_line = file.lines.line(offset);
            let next_line = file.lines.line(token.span.start);
            if line.contains("@phpstan-ignore-next-line") {
                if diagnostic.line == comment_line + 1 {
                    return true;
                }
            } else if line.contains("@phpstan-ignore-line") {
                if diagnostic.line == comment_line {
                    return true;
                }
            } else if let Some((_, identifiers)) = line.split_once("@phpstan-ignore") {
                let code_before = file
                    .source
                    .lines()
                    .nth(comment_line - 1)
                    .is_some_and(|l| !l.trim_start().starts_with(['/', '*', '#']));
                let target = if code_before { comment_line } else { next_line };
                if diagnostic.line == target && has_identifier(identifiers, &diagnostic.identifier)
                {
                    return true;
                }
            } else if let Some((_, identifiers)) = line.split_once("@phera-ignore") {
                if (diagnostic.line == comment_line || diagnostic.line == next_line)
                    && (identifiers.trim().trim_end_matches("*/").is_empty()
                        || has_identifier(identifiers, &diagnostic.identifier))
                {
                    return true;
                }
            } else if let Some((_, identifiers)) = line.split_once("@psalm-suppress") {
                let range = declaration_end(file, token.span.start);
                if diagnostic.line >= next_line
                    && diagnostic.line <= range
                    && identifiers
                        .split_whitespace()
                        .next()
                        .is_some_and(|id| psalm_identifier(id) == diagnostic.identifier)
                {
                    return true;
                }
            }
        }
        end = token.span.end;
        if token.kind == TokenKind::Eof {
            break;
        }
    }
    false
}

fn has_identifier(text: &str, identifier: &str) -> bool {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .any(|part| part == identifier)
}

fn psalm_identifier(id: &str) -> &str {
    match id {
        "UndefinedClass" => "class.notFound",
        "UndefinedFunction" => "function.notFound",
        "UndefinedMethod" => "method.notFound",
        "UndefinedPropertyFetch" | "UndefinedPropertyAssignment" => "property.notFound",
        "UndefinedConstant" => "constant.notFound",
        "UndefinedVariable" | "PossiblyUndefinedVariable" | "InvalidScope" => "variable.undefined",
        "TooManyArguments" | "TooFewArguments" => "arguments.count",
        "InvalidNamedArgument" => "argument.unknown",
        "InvalidReturnStatement" => "return.void",
        "InvalidReturnType" => "return.missing",
        "AbstractInstantiation" => "new.abstract",
        "InvalidStaticInvocation" => "method.staticCall",
        "UnimplementedInterfaceMethod" => "class.missingMethod",
        _ => "",
    }
}

fn declaration_end(file: &ParsedFile, start: usize) -> usize {
    let mut pending = vec![file.program.statements.as_slice()];
    while let Some(statements) = pending.pop() {
        for statement in statements {
            if statement.span.start == start {
                return file.lines.line(statement.span.end);
            }
            pending.extend(crate::collect::child_blocks(statement));
            if let phera_ast::StatementKind::Namespace { statements, .. } = &statement.kind {
                pending.push(statements);
            }
            if let phera_ast::StatementKind::Class(class) = &statement.kind {
                for member in &class.members {
                    if member.span.start == start {
                        return file.lines.line(member.span.end);
                    }
                }
            }
        }
    }
    file.lines.line(start)
}
