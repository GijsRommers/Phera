//! `PhpToken::tokenize("<?php\r\necho 1;")` gives `echo` line 2 and byte position 7.

use phera_extension_api::{
    CompileDiagnosticSite, Context, NativeError, NativeResult, Param, ParamType, PhpArray, Value,
};
use phera_lexer::{Lexer, ScanMode, ScanReport, TokenKind};

use crate::{ids::TOKEN_PARSE, parse, token};

pub(crate) const PARAMETERS: &[Param] = &[
    Param::new("code", ParamType::String),
    Param::new("flags", ParamType::Int).optional(),
];

pub(crate) struct SourceToken<'a> {
    pub(crate) kind: TokenKind,
    pub(crate) id: i64,
    pub(crate) text: &'a [u8],
    pub(crate) line: i64,
    pub(crate) pos: usize,
}

pub(crate) fn tokenize(
    context: &mut dyn Context,
    code: &[u8],
    flags: i64,
    mut make_token: impl FnMut(&mut dyn Context, SourceToken<'_>) -> NativeResult,
) -> NativeResult {
    let mode = if flags & TOKEN_PARSE != 0 {
        ScanMode::Parsed
    } else {
        ScanMode::Raw
    };
    let scanned = scan(code, mode)?;
    let mut tokens = scanned.tokens;
    match mode {
        ScanMode::Parsed => parse::validate(context, code, &mut tokens, scanned.report)?,
        ScanMode::Raw => {
            for (_, diagnostic) in scanned.report.diagnostics {
                context.compile_diagnostic(&diagnostic, CompileDiagnosticSite::Caller);
            }
        }
    }
    let mut output = PhpArray::new();
    for token in tokens {
        output.push(make_token(context, token)?)?;
    }
    Ok(Value::from(output))
}

struct Scan<'a> {
    tokens: Vec<SourceToken<'a>>,
    report: ScanReport,
}

fn scan(code: &[u8], mode: ScanMode) -> Result<Scan<'_>, NativeError> {
    let mut lexer = Lexer::for_tokenizer(code)
        .with_short_open_tags(true)
        .with_diagnostics(mode);
    let mut tokens = Vec::new();
    let mut line = 1;
    let mut halt_remaining = None;
    let mut halt_line = None;
    loop {
        let token = lexer
            .next_token()
            .map_err(|error| NativeError::error(format!("Tokenizer scanner failed: {error:?}")))?;
        let kind = token.kind;
        if kind == TokenKind::Eof {
            return Ok(Scan {
                tokens,
                report: lexer.into_diagnostics(),
            });
        }
        let pos = token.span.start;
        let text = &code[pos..token.span.end];
        let id = token::id(kind, text);
        tokens.push(SourceToken {
            kind,
            id,
            text,
            line: halt_line.take().unwrap_or(line),
            pos,
        });
        if !token::ignorable(id) {
            match &mut halt_remaining {
                Some(0) => {}
                Some(remaining) => {
                    *remaining -= 1;
                    if *remaining == 0 {
                        halt_line = Some(line);
                    }
                }
                None if kind == TokenKind::HaltCompiler => halt_remaining = Some(3),
                None => {}
            }
        }
        line += newlines(text);
    }
}

pub(crate) fn newlines(text: &[u8]) -> i64 {
    let mut lines = 0;
    let mut previous = 0;
    for &byte in text {
        if byte == b'\r' || (byte == b'\n' && previous != b'\r') {
            lines += 1;
        }
        previous = byte;
    }
    lines
}
