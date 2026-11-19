//! Adoption comments: `missing(); // @phpstan-ignore function.notFound`.
//! Comments are read only from lexer gaps, so a string containing an ignore is harmless.

use crate::{diagnostic::Diagnostic, project::ParsedFile};
use phera_lexer::{Lexer, TokenKind};

enum Directive<'a> {
    All,
    Identifiers(&'a str),
}

pub fn filter(file: &ParsedFile, diagnostics: &mut Vec<Diagnostic>) {
    if diagnostics.is_empty()
        || !file.source.contains("-ignore") && !file.source.contains("@psalm-suppress")
    {
        return;
    }
    // A file with many diagnostics still needs just one walk over its tokens.
    let mut lines = std::collections::BTreeMap::<usize, Vec<Directive<'_>>>::new();
    let mut ranges = Vec::new();
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
                lines
                    .entry(comment_line + 1)
                    .or_default()
                    .push(Directive::All);
            } else if line.contains("@phpstan-ignore-line") {
                lines.entry(comment_line).or_default().push(Directive::All);
            } else if let Some((_, identifiers)) = line.split_once("@phpstan-ignore") {
                let start = file.source[..offset]
                    .rfind('\n')
                    .map_or(0, |newline| newline + 1);
                let code_before = !file.source[start..]
                    .trim_start()
                    .starts_with(['/', '*', '#']);
                let target = if code_before { comment_line } else { next_line };
                lines
                    .entry(target)
                    .or_default()
                    .push(Directive::Identifiers(identifiers));
            } else if let Some((_, identifiers)) = line.split_once("@phera-ignore") {
                for target in [comment_line, next_line] {
                    let directive = if identifiers.trim().trim_end_matches("*/").is_empty() {
                        Directive::All
                    } else {
                        Directive::Identifiers(identifiers)
                    };
                    lines.entry(target).or_default().push(directive);
                }
            } else if let Some((_, identifiers)) = line.split_once("@psalm-suppress")
                && let Some(identifier) = identifiers.split_whitespace().next()
            {
                ranges.push((
                    next_line..=declaration_end(file, token.span.start),
                    identifier,
                ));
            }
        }
        end = token.span.end;
        if token.kind == TokenKind::Eof {
            break;
        }
    }
    diagnostics.retain(|diagnostic| {
        !lines.get(&diagnostic.line).is_some_and(|directives| {
            directives.iter().any(|directive| match directive {
                Directive::All => true,
                Directive::Identifiers(identifiers) => {
                    has_identifier(identifiers, &diagnostic.identifier)
                }
            })
        }) && !ranges.iter().any(|(range, identifier)| {
            range.contains(&diagnostic.line) && psalm_matches(identifier, &diagnostic.identifier)
        })
    });
}

fn has_identifier(text: &str, identifier: &str) -> bool {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .any(|part| part == identifier)
}

fn psalm_identifier(id: &str) -> &str {
    match id {
        "UndefinedClass" | "UndefinedInterface" | "UndefinedTrait" => "class.notFound",
        "UndefinedFunction" => "function.notFound",
        "UndefinedMethod" | "UndefinedInterfaceMethod" => "method.notFound",
        "UndefinedPropertyFetch"
        | "UndefinedPropertyAssignment"
        | "UndefinedThisPropertyFetch"
        | "UndefinedThisPropertyAssignment" => "property.notFound",
        "UndefinedConstant" => "constant.notFound",
        "UndefinedVariable"
        | "PossiblyUndefinedVariable"
        | "UndefinedGlobalVariable"
        | "PossiblyUndefinedGlobalVariable"
        | "InvalidScope" => "variable.undefined",
        "TooManyArguments" | "TooFewArguments" => "arguments.count",
        "InvalidNamedArgument" => "argument.unknown",
        "InvalidReturnStatement" => "return.void",
        "InvalidReturnType" => "return.missing",
        "AbstractInstantiation" => "new.abstract",
        "InvalidStaticInvocation" | "NonStaticSelfCall" => "method.staticCall",
        "UnimplementedInterfaceMethod" | "UnimplementedAbstractMethod" => "method.abstract",
        _ => "",
    }
}

fn declaration_end(file: &ParsedFile, start: usize) -> usize {
    let mut pending = vec![file.program.statements.as_slice()];
    while let Some(statements) = pending.pop() {
        for statement in statements {
            let attributes: &[phera_ast::Attribute] = match &statement.kind {
                phera_ast::StatementKind::Function(function) => &function.attributes,
                phera_ast::StatementKind::Class(class) => &class.attributes,
                phera_ast::StatementKind::Const { attributes, .. } => attributes,
                _ => &[],
            };
            if begins_at(file, start, statement.span.start, attributes) {
                return file.lines.line(statement.span.end);
            }
            pending.extend(crate::collect::child_blocks(statement));
            if let phera_ast::StatementKind::Namespace { statements, .. } = &statement.kind {
                pending.push(statements);
            }
            if let phera_ast::StatementKind::Class(class) = &statement.kind {
                for member in &class.members {
                    let attributes: &[phera_ast::Attribute] = match &member.kind {
                        phera_ast::ClassMemberKind::Method(method) => &method.attributes,
                        phera_ast::ClassMemberKind::Property { attributes, .. }
                        | phera_ast::ClassMemberKind::Constant { attributes, .. }
                        | phera_ast::ClassMemberKind::EnumCase { attributes, .. } => attributes,
                        _ => &[],
                    };
                    if begins_at(file, start, member.span.start, attributes) {
                        return file.lines.line(member.span.end);
                    }
                }
            }
        }
    }
    file.lines.line(start)
}

fn psalm_matches(id: &str, identifier: &str) -> bool {
    psalm_identifier(id) == identifier
        || (matches!(id, "InvalidReturnType" | "InvalidReturnStatement")
            && matches!(identifier, "return.void" | "return.missing"))
        || (id == "UndefinedConstant" && identifier == "classConstant.notFound")
        || (matches!(
            id,
            "UndefinedPropertyFetch"
                | "UndefinedPropertyAssignment"
                | "UndefinedThisPropertyFetch"
                | "UndefinedThisPropertyAssignment"
        ) && identifier == "staticProperty.notFound")
        || (id == "AbstractInstantiation" && identifier == "new.interface")
}

fn begins_at(
    file: &ParsedFile,
    start: usize,
    declaration: usize,
    attributes: &[phera_ast::Attribute],
) -> bool {
    start == declaration
        || attributes.first().is_some_and(|attribute| {
            file.source
                .get(start..attribute.span.start)
                .is_some_and(|prefix| prefix.trim_start().starts_with("#["))
        })
}
