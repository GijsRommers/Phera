//! `token_get_all("<?php X::class;", TOKEN_PARSE)` tags `class` as `T_STRING`.

use phera_extension_api::{CompileDiagnosticSite, Context, NativeError};
use phera_lexer::{ScanReport, TokenKind};

use crate::{errors, feedback, parse_source::ParserInput, stream::SourceToken};

pub(crate) fn validate(
    context: &mut dyn Context,
    code: &[u8],
    tokens: &mut [SourceToken<'_>],
    report: ScanReport,
) -> Result<(), NativeError> {
    let input = ParserInput::from_tokens(tokens);
    let result = phera_parser::parse_bytes(&input.source);
    let error_at = result
        .as_ref()
        .err()
        .and_then(|error| input.token_at(error.span.start));
    let last = error_at.map_or(code.len(), |index| {
        tokens[index].pos + tokens[index].text.len()
    });
    let nesting_error = report
        .nesting_error
        .filter(|error| error_at.is_none_or(|index| error.span.start <= tokens[index].pos));
    let last = nesting_error
        .as_ref()
        .map_or(last, |error| last.min(error.span.end));
    for (offset, diagnostic) in report.diagnostics {
        if offset < last {
            context.compile_diagnostic(&diagnostic, CompileDiagnosticSite::Source(""));
        }
    }
    if let Some(error) = nesting_error {
        return Err(errors::exception(
            "ParseError",
            error.message,
            error.line as i64,
        ));
    }
    if let Some(token) = error_at.and_then(|index| tokens.get(index))
        && token.kind == TokenKind::FloatCast
        && token.text[1..token.text.len() - 1]
            .trim_ascii()
            .eq_ignore_ascii_case(b"real")
    {
        return Err(errors::exception(
            "ParseError",
            "The (real) cast has been removed, use (float) instead",
            token.line,
        ));
    }
    if let Err(error) = result {
        if error.message == "Cannot use \"<?=\" as an identifier"
            && let Some(token) = error_at.and_then(|index| tokens.get(index))
        {
            return Err(errors::exception("ParseError", error.message, token.line));
        }
        return Err(errors::translate(error, error_at, tokens, code));
    }
    validate_halt_scope(tokens)?;
    feedback::retag(tokens);
    Ok(())
}

fn validate_halt_scope(tokens: &[SourceToken<'_>]) -> Result<(), NativeError> {
    use TokenKind::*;

    let mut blocks = 0usize;
    for token in tokens {
        match token.kind {
            LeftBrace | CurlyOpen | DollarCurlyOpen => blocks += 1,
            RightBrace => blocks = blocks.saturating_sub(1),
            HaltCompiler if blocks > 0 => {
                return Err(errors::exception(
                    "CompileError",
                    "__HALT_COMPILER() can only be used from the outermost scope",
                    token.line,
                ));
            }
            _ => {}
        }
    }
    Ok(())
}
