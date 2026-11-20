//! Adoption comments: `missing(); // @phpstan-ignore function.notFound`.
//! Comments are read only from lexer gaps, so a string containing an ignore is harmless.

use crate::{diagnostic::Diagnostic, project::ParsedFile};
use phera_lexer::{Lexer, TokenKind};

enum Directive<'a> {
    All,
    Identifiers(&'a str),
}

pub fn filter(file: &ParsedFile, diagnostics: &mut Vec<Diagnostic>, report_unmatched: bool) {
    if (diagnostics.is_empty() && !report_unmatched)
        || !file.source.contains("-ignore") && !file.source.contains("@psalm-suppress")
    {
        return;
    }
    // A file with many diagnostics still needs just one walk over its tokens.
    let mut lines = std::collections::BTreeMap::<usize, Vec<Directive<'_>>>::new();
    let mut ranges = Vec::new();
    let mut phpstan_ignores = Vec::new();
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
                if report_unmatched {
                    phpstan_ignores.push((comment_line + 1, None));
                }
            } else if line.contains("@phpstan-ignore-line") {
                lines.entry(comment_line).or_default().push(Directive::All);
                if report_unmatched {
                    phpstan_ignores.push((comment_line, None));
                }
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
                if report_unmatched {
                    phpstan_ignores.extend(
                        identifiers
                            .split('(')
                            .next()
                            .unwrap_or_default()
                            .split(',')
                            .filter_map(|item| item.split_whitespace().next())
                            .filter(|id| id.contains('.'))
                            .map(|id| (target, Some(id))),
                    );
                }
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
    let unmatched: Vec<_> = phpstan_ignores
        .into_iter()
        .filter(|(line, identifier)| {
            !diagnostics.iter().any(|diagnostic| {
                diagnostic.line == *line && identifier.is_none_or(|id| id == diagnostic.identifier)
            })
        })
        .collect();
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
    diagnostics.extend(unmatched.into_iter().map(|(line, identifier)| {
        let (identifier, message) = if let Some(id) = identifier {
            (
                "ignore.unmatchedIdentifier",
                format!("No error with identifier {id} is reported on line {line}."),
            )
        } else {
            (
                "ignore.unmatchedLine",
                format!("No error is reported on line {line}."),
            )
        };
        Diagnostic {
            file: file.path.clone(),
            line,
            identifier: identifier.into(),
            message,
            severity: Default::default(),
        }
    }));
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

/// A line comment can suppress a finding on its own line or the line after it.
pub(crate) struct LintDirective<'a> {
    pub name: &'a str,
    pub line: usize,
    pub file: bool,
    pub used: bool,
}

pub(crate) fn lint_directives<'a>(
    source: &'a [u8],
    tokens: &[phera_lexer::Token],
    lines: &phera_source::LineTable,
) -> Vec<LintDirective<'a>> {
    tokens
        .iter()
        .filter_map(|token| {
            if token.kind != TokenKind::Comment {
                return None;
            }
            let comment = source
                .get(token.span.start..token.span.end)?
                .strip_prefix(b"//")?;
            let comment = std::str::from_utf8(comment).ok()?.trim_start();
            let mut words = comment.split_whitespace();
            let file = match words.next()? {
                "phera-ignore-file" => true,
                "phera-ignore" => false,
                _ => return None,
            };
            Some(LintDirective {
                name: words.next()?,
                line: lines.line(token.span.start),
                file,
                used: false,
            })
        })
        .collect()
}

pub(crate) fn is_lint_suppressed(
    diagnostic: &Diagnostic,
    directives: &mut [LintDirective<'_>],
) -> bool {
    let matched = directives
        .iter_mut()
        .filter(|directive| {
            directive.name == diagnostic.identifier
                && (directive.file
                    || directive.line == diagnostic.line
                    || directive.line + 1 == diagnostic.line)
        })
        .min_by_key(|directive| (directive.file, directive.line != diagnostic.line));
    if let Some(directive) = matched {
        directive.used = true;
        true
    } else {
        false
    }
}
