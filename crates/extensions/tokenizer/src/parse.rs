//! `token_get_all("<?php X::class;", TOKEN_PARSE)` tags `class` as `T_STRING`.

use phera_extension_api::{Context, NativeError};
use phera_lexer::TokenKind;

use crate::{errors, feedback, parse_source::ParserInput, stream::SourceToken};

pub(crate) fn validate(
    context: &mut dyn Context,
    code: &[u8],
    tokens: &mut [SourceToken<'_>],
) -> Result<(), NativeError> {
    let input = ParserInput::from_tokens(tokens);
    let result = phera_parser::parse(&input.source);
    let error_at = result
        .as_ref()
        .err()
        .and_then(|error| input.token_at(error.span.start));
    for token in tokens
        .iter()
        .take(error_at.map_or(tokens.len(), |index| index + 1))
    {
        check_cast(context, token)?;
    }
    result.map_err(|error| errors::translate(error, error_at, tokens, code))?;
    validate_halt_scope(tokens)?;
    feedback::retag(tokens);
    Ok(())
}

fn check_cast(context: &mut dyn Context, token: &SourceToken<'_>) -> Result<(), NativeError> {
    use TokenKind::*;
    if !matches!(token.kind, IntCast | BoolCast | FloatCast | StringCast) {
        return Ok(());
    }
    let spelling = token.text[1..token.text.len() - 1]
        .trim_ascii()
        .to_ascii_lowercase();
    let (old, new) = match spelling.as_slice() {
        b"integer" => ("integer", "int"),
        b"double" => ("double", "float"),
        b"boolean" => ("boolean", "bool"),
        b"binary" => ("binary", "string"),
        b"real" => {
            return Err(errors::exception(
                "ParseError",
                "The (real) cast has been removed, use (float) instead",
                token.line,
            ));
        }
        _ => return Ok(()),
    };
    context.deprecated(format!(
        "Non-canonical cast ({old}) is deprecated, use the ({new}) cast instead"
    ));
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
