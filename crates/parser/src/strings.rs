//! String literals: escape decoding, interpolated strings and heredocs/nowdocs.
//!
//! The lexer hands over complete non-interpolated strings as one `String`
//! token (quotes included), and interpolated ones as a `DoubleQuote` /
//! `StartHeredoc` run of `StringPart`s and interpolation tokens. Escapes are
//! resolved here, into bytes, the way PHP resolves them: PHP strings are byte
//! strings, so `"\xff"` is a single byte.

use phera_ast::{
    DollarCurlyForm, Expression, ExpressionKind, Identifier, Literal, MemberName, StringPart,
    Variable,
};
use phera_lexer::TokenKind;
use phera_source::Span;

use crate::error::ParseError;
use crate::parser::{Parser, is_word_token};

impl Parser<'_> {
    /// Parses a `String` token (a complete literal with its quotes) into a
    /// [`Literal::String`].
    pub(crate) fn parse_string_token(&mut self) -> Result<Expression, ParseError> {
        let token = self.bump()?;
        let text = self.bytes_of(&token);

        let (quote, body) = split_quoted(text);
        let bytes = if quote == b'\'' {
            decode_single_quoted(body)
        } else {
            decode_double_quoted(body, false, token.span)?
        };

        Ok(Expression {
            kind: ExpressionKind::Literal(Literal::String(bytes)),
            span: token.span,
        })
    }

    /// Parses an interpolated `"..."` string, starting at the opening
    /// `DoubleQuote`. With no interpolation at all this is a plain string
    /// literal.
    pub(crate) fn parse_interpolated_string(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect(TokenKind::DoubleQuote, "double quote")?.span;
        let mut parts: Vec<StringPart> = Vec::new();

        loop {
            match self.kind() {
                TokenKind::DoubleQuote => {
                    self.bump()?;
                    break;
                }
                TokenKind::StringPart => {
                    let token = self.bump()?;
                    let bytes = decode_double_quoted(self.bytes_of(&token), false, token.span)?;
                    parts.push(StringPart::Literal(bytes, token.span));
                }
                TokenKind::Variable => {
                    parts.push(StringPart::Expression(self.parse_encaps_var()?));
                }
                TokenKind::CurlyOpen => {
                    self.bump()?;
                    let expression = self.parse_expr(0)?;
                    self.expect(TokenKind::RightBrace, "'}'")?;
                    parts.push(StringPart::Expression(expression));
                }
                TokenKind::DollarCurlyOpen => parts.push(self.parse_dollar_curly()?),
                _ => {
                    return Err(
                        self.error(format!("unexpected {} in string", self.describe_current()))
                    );
                }
            }
        }

        Ok(self.finish_string_parts(parts, start))
    }

    /// Parses a heredoc or nowdoc, starting at the `StartHeredoc` token.
    pub(crate) fn parse_heredoc(&mut self) -> Result<Expression, ParseError> {
        let start_token = self.expect(TokenKind::StartHeredoc, "heredoc start")?;
        let (label, nowdoc) = parse_heredoc_start(self.bytes_of(&start_token));

        let mut parts: Vec<RawPart> = Vec::new();

        let indent = loop {
            match self.kind() {
                TokenKind::EndHeredoc => {
                    let token = self.bump()?;
                    let end_text = self.bytes_of(&token);
                    break &end_text[..end_text.len() - label.len()];
                }
                TokenKind::StringPart => {
                    let token = self.bump()?;
                    parts.push(RawPart::Literal(self.bytes_of(&token), token.span));
                }
                TokenKind::Variable if !nowdoc => {
                    parts.push(RawPart::Interpolation(StringPart::Expression(
                        self.parse_encaps_var()?,
                    )));
                }
                TokenKind::CurlyOpen if !nowdoc => {
                    self.bump()?;
                    let expression = self.parse_expr(0)?;
                    self.expect(TokenKind::RightBrace, "'}'")?;
                    parts.push(RawPart::Interpolation(StringPart::Expression(expression)));
                }
                TokenKind::DollarCurlyOpen if !nowdoc => {
                    parts.push(RawPart::Interpolation(self.parse_dollar_curly()?));
                }
                _ => {
                    return Err(
                        self.error(format!("unexpected {} in heredoc", self.describe_current()))
                    );
                }
            }
        };

        let body = self.finish_heredoc(parts, indent, nowdoc)?;
        Ok(Expression {
            kind: body.into_kind(),
            span: self.span_since(start_token.span),
        })
    }

    /// `${name}` (with optional index) names a variable directly. The expression
    /// form computes the name, as `${$name}` does.
    fn parse_dollar_curly(&mut self) -> Result<StringPart, ParseError> {
        self.bump()?;
        let (kind, span, form) = if self.kind() == TokenKind::StringVarname {
            let token = self.bump()?;
            let variable =
                ExpressionKind::Variable(Variable::Named(self.text_of(&token).to_string()));
            let kind = if self.eat(TokenKind::LeftBracket)? {
                let index = self.parse_expr(0)?;
                self.expect(TokenKind::RightBracket, "']'")?;
                ExpressionKind::Index {
                    target: Box::new(Expression {
                        kind: variable,
                        span: token.span,
                    }),
                    index: Some(Box::new(index)),
                }
            } else {
                variable
            };
            (kind, token.span, DollarCurlyForm::Name)
        } else {
            let name = self.parse_expr(0)?;
            let span = name.span;
            (
                ExpressionKind::Variable(Variable::Dynamic(Box::new(name))),
                span,
                DollarCurlyForm::Expression,
            )
        };
        self.expect(TokenKind::RightBrace, "'}'")?;
        Ok(StringPart::DollarCurly(Expression { kind, span }, form))
    }

    /// One interpolated variable inside a string: `$a`, `$a[...]`, `$a->b`,
    /// `$a?->b`.
    pub(crate) fn parse_encaps_var(&mut self) -> Result<Expression, ParseError> {
        let token = self.expect(TokenKind::Variable, "variable")?;
        let name = self.text_of(&token)[1..].to_string();
        let mut expr = Expression {
            kind: ExpressionKind::Variable(Variable::Named(name)),
            span: token.span,
        };

        match self.kind() {
            TokenKind::LeftBracket => {
                self.bump()?;
                let index = self.parse_encaps_offset()?;
                self.expect(TokenKind::RightBracket, "']'")?;
                expr = Expression {
                    kind: ExpressionKind::Index {
                        target: Box::new(expr),
                        index: index.map(Box::new),
                    },
                    span: self.span_since(token.span),
                };
            }
            TokenKind::Arrow | TokenKind::NullsafeArrow => {
                let nullsafe = self.kind() == TokenKind::NullsafeArrow;
                self.bump()?;
                let name_span = self.peek_span();
                if !is_word_token(self.kind()) {
                    return Err(self.error(format!(
                        "expected a property name, found {}",
                        self.describe_current()
                    )));
                }
                let name = self.text().to_string();
                self.bump()?;
                expr = Expression {
                    kind: ExpressionKind::Property {
                        target: Box::new(expr),
                        name: MemberName::Identifier(Identifier {
                            name,
                            span: name_span,
                        }),
                        nullsafe,
                    },
                    span: self.span_since(token.span),
                };
            }
            _ => {}
        }

        Ok(expr)
    }

    /// The offset in `"$a[...]"`: a number, a bare name (a string key), a
    /// variable, or `-` followed by a number.
    fn parse_encaps_offset(&mut self) -> Result<Option<Expression>, ParseError> {
        let span = self.peek_span();

        let literal = match self.kind() {
            TokenKind::RightBracket => return Ok(None),
            TokenKind::NumString => {
                let token = self.bump()?;
                num_string_literal(&self.text_of(&token), token.span)
            }
            TokenKind::Minus => {
                self.bump()?;
                let token = self.expect(TokenKind::NumString, "a number")?;
                let text = self.text_of(&token);
                match canonical_decimal(&text) {
                    Some(value) => Expression {
                        kind: ExpressionKind::Literal(Literal::Int(-value)),
                        span,
                    },
                    None => Expression {
                        kind: ExpressionKind::Literal(Literal::String(
                            format!("-{text}").into_bytes(),
                        )),
                        span,
                    },
                }
            }
            TokenKind::Variable => {
                let token = self.bump()?;
                Expression {
                    kind: ExpressionKind::Variable(Variable::Named(
                        self.text_of(&token)[1..].to_string(),
                    )),
                    span: token.span,
                }
            }
            kind if is_word_token(kind) => {
                let token = self.bump()?;
                Expression {
                    kind: ExpressionKind::Literal(Literal::String(self.bytes_of(&token).to_vec())),
                    span: token.span,
                }
            }
            _ => {
                return Err(self.error(format!(
                    "unexpected {} in string offset",
                    self.describe_current()
                )));
            }
        };

        Ok(Some(literal))
    }

    /// Builds the final string expression from parts, collapsing to a plain
    /// literal when nothing is interpolated.
    fn finish_string_parts(&self, parts: Vec<StringPart>, start: Span) -> Expression {
        if parts
            .iter()
            .all(|part| matches!(part, StringPart::Literal(..)))
        {
            let mut bytes = Vec::new();
            for part in parts {
                if let StringPart::Literal(literal, _) = part {
                    bytes.extend_from_slice(&literal);
                }
            }
            Expression {
                kind: ExpressionKind::Literal(Literal::String(bytes)),
                span: self.span_since(start),
            }
        } else {
            Expression {
                kind: ExpressionKind::InterpolatedString(parts),
                span: self.span_since(start),
            }
        }
    }

    /// Applies flexible-heredoc indentation removal to the raw literal parts,
    /// drops the newline before the closing label, decodes escapes, and
    /// builds the final expression.
    fn finish_heredoc(
        &self,
        parts: Vec<RawPart<'_>>,
        indent: &[u8],
        nowdoc: bool,
    ) -> Result<Expression, ParseError> {
        // PHP 7.3 flexible heredoc rules: the closing label's indentation must
        // be a prefix of every non-blank body line, and tabs and spaces may
        // not be mixed.
        if indent.contains(&b' ') && indent.contains(&b'\t') {
            return Err(ParseError::new(
                "invalid indentation - tabs and spaces cannot be mixed",
                Span::new(0, 0),
            ));
        }
        let indent_style = if indent.contains(&b'\t') { b'\t' } else { b' ' };

        let mut result: Vec<StringPart> = Vec::new();
        let mut at_line_start = true;
        let last_literal = parts
            .iter()
            .rposition(|part| matches!(part, RawPart::Literal(..)));

        for (index, part) in parts.into_iter().enumerate() {
            match part {
                RawPart::Interpolation(part) => {
                    let span = match &part {
                        StringPart::Expression(expression)
                        | StringPart::DollarCurly(expression, _) => expression.span,
                        StringPart::Literal(_, span) => *span,
                    };
                    if at_line_start && !indent.is_empty() {
                        return Err(ParseError::new(
                            format!(
                                "invalid body indentation level (expecting an indentation level of at least {})",
                                indent.len()
                            ),
                            span,
                        ));
                    }
                    at_line_start = false;
                    result.push(part);
                }
                RawPart::Literal(raw, span) => {
                    let mut text =
                        strip_heredoc_indent(raw, indent, &mut at_line_start, indent_style, span)?;
                    if Some(index) == last_literal {
                        let newline = if text.ends_with(b"\r\n") {
                            2
                        } else {
                            usize::from(ends_in_newline(&text))
                        };
                        text.truncate(text.len() - newline);
                    }
                    let bytes = if nowdoc {
                        text
                    } else {
                        decode_double_quoted(&text, true, span)?
                    };
                    result.push(StringPart::Literal(bytes, span));
                }
            }
        }

        Ok(self.finish_string_parts(result, Span::new(0, 0)))
    }
}

enum RawPart<'a> {
    Literal(&'a [u8], Span),
    Interpolation(StringPart),
}

/// Splits a `String` token's text into its quote character and body.
fn split_quoted(text: &[u8]) -> (u8, &[u8]) {
    let prefix = usize::from(matches!(text.first(), Some(b'b' | b'B')));
    let quote = text[prefix];
    (quote, &text[prefix + 1..text.len() - 1])
}

/// The label and nowdoc flag of a `StartHeredoc` token's text, like
/// `<<<EOT\n`, `<<<"EOT"\n`, `<<<'EOT'\n` or `b<<<EOT\n`.
fn parse_heredoc_start(text: &[u8]) -> (&[u8], bool) {
    let prefix = usize::from(matches!(text.first(), Some(b'b' | b'B')));
    let text = text[prefix + 3..].trim_ascii_start();
    let (quote, rest) = match text {
        [quote @ (b'\'' | b'"'), rest @ ..] => (Some(*quote), rest),
        _ => (None, text),
    };
    let end = rest
        .iter()
        .position(|byte| byte.is_ascii() && !byte.is_ascii_alphanumeric() && *byte != b'_')
        .unwrap_or(rest.len());
    (&rest[..end], quote == Some(b'\''))
}

/// Removes the closing label's indentation from the start of every line of a
/// heredoc body part, validating PHP's flexible-heredoc indentation rules
/// (mirrors `strip_multiline_string_indentation` in PHP's scanner): the first
/// `indent.len()` characters of every line must be whitespace of the same
/// style as the closing label's indentation; whitespace-only lines are exempt.
/// `at_line_start` tracks whether the next byte starts a line (the body starts
/// at a line start; an interpolation ends mid-line).
fn strip_heredoc_indent(
    text: &[u8],
    indent: &[u8],
    at_line_start: &mut bool,
    indent_style: u8,
    span: Span,
) -> Result<Vec<u8>, ParseError> {
    let validate_line = |bytes: &[u8]| -> Result<(), ParseError> {
        for i in 0..indent.len() {
            match bytes.get(i) {
                // Whitespace-only (or empty) line: exempt.
                None | Some(b'\n') | Some(b'\r') => return Ok(()),
                Some(&c) if c == b' ' || c == b'\t' => {
                    if c != indent_style {
                        return Err(ParseError::new(
                            "invalid indentation - tabs and spaces cannot be mixed",
                            span,
                        ));
                    }
                }
                Some(_) => {
                    return Err(ParseError::new(
                        format!(
                            "invalid body indentation level (expecting an indentation level of at least {})",
                            indent.len()
                        ),
                        span,
                    ));
                }
            }
        }
        Ok(())
    };

    if indent.is_empty() {
        *at_line_start = ends_in_newline(text);
        return Ok(text.to_vec());
    }

    let mut out = Vec::with_capacity(text.len());
    let mut rest = text;
    let mut line_start = *at_line_start;

    loop {
        if line_start {
            let line_end = rest
                .iter()
                .position(|byte| matches!(byte, b'\n' | b'\r'))
                .unwrap_or(rest.len());
            validate_line(&rest[..line_end])?;
            if rest.starts_with(indent) {
                rest = &rest[indent.len()..];
            }
        }

        match rest.iter().position(|byte| matches!(byte, b'\n' | b'\r')) {
            Some(newline) => {
                let (line, after) = rest.split_at(newline);
                out.extend_from_slice(line);
                let newline_len = if after.starts_with(b"\r\n") { 2 } else { 1 };
                out.extend_from_slice(&after[..newline_len]);
                rest = &after[newline_len..];
                line_start = true;
            }
            None => {
                out.extend_from_slice(rest);
                break;
            }
        }
    }

    *at_line_start = ends_in_newline(text);
    Ok(out)
}

/// Resolves the escapes of a single-quoted string body: only `\\` and `\'`.
fn decode_single_quoted(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && matches!(bytes.get(i + 1), Some(b'\\' | b'\'')) {
            out.push(bytes[i + 1]);
            i += 2;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }

    out
}

/// Resolves the escapes of a double-quoted string or heredoc body. In a
/// heredoc `\"` stays two characters; every other double-quote escape works
/// the same. Unknown escapes keep their backslash.
fn decode_double_quoted(bytes: &[u8], heredoc: bool, span: Span) -> Result<Vec<u8>, ParseError> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte != b'\\' {
            out.push(byte);
            i += 1;
            continue;
        }

        i += 1;
        let Some(&escape) = bytes.get(i) else {
            out.push(b'\\');
            break;
        };

        match escape {
            b'n' => simple(&mut out, b'\n', &mut i),
            b'r' => simple(&mut out, b'\r', &mut i),
            b't' => simple(&mut out, b'\t', &mut i),
            b'v' => simple(&mut out, 0x0B, &mut i),
            b'e' => simple(&mut out, 0x1B, &mut i),
            b'f' => simple(&mut out, 0x0C, &mut i),
            b'\\' => simple(&mut out, b'\\', &mut i),
            b'$' => simple(&mut out, b'$', &mut i),
            b'"' if !heredoc => simple(&mut out, b'"', &mut i),
            b'0'..=b'7' => {
                // Octal escape: up to three digits, value taken modulo 256.
                let mut value = 0u32;
                let mut digits = 0;
                while digits < 3 && bytes.get(i).is_some_and(|b| (b'0'..=b'7').contains(b)) {
                    value = value * 8 + u32::from(bytes[i] - b'0');
                    i += 1;
                    digits += 1;
                }
                out.push((value & 0xFF) as u8);
            }
            b'x' => {
                i += 1;
                let mut value = 0u32;
                let mut digits = 0;
                while digits < 2 && bytes.get(i).is_some_and(u8::is_ascii_hexdigit) {
                    value = value * 16 + hex_value(bytes[i]);
                    i += 1;
                    digits += 1;
                }
                if digits == 0 {
                    out.push(b'\\');
                    out.push(b'x');
                } else {
                    out.push(value as u8);
                }
            }
            b'u' if bytes.get(i + 1) == Some(&b'{') => {
                i += 2;
                let start = i;
                while bytes.get(i).is_some_and(u8::is_ascii_hexdigit) {
                    i += 1;
                }
                let invalid = |message: &str| {
                    ParseError::new(
                        format!("invalid UTF-8 codepoint escape sequence{message}"),
                        span,
                    )
                };
                if bytes.get(i) != Some(&b'}') || i == start {
                    return Err(invalid(""));
                }
                let codepoint = bytes[start..i]
                    .iter()
                    .try_fold(0u32, |value, &digit| {
                        value.checked_mul(16)?.checked_add(hex_value(digit))
                    })
                    .ok_or_else(|| invalid(": codepoint too large"))?;
                i += 1; // consume '}'
                if codepoint > 0x10FFFF {
                    return Err(invalid(": codepoint too large"));
                }
                // Surrogate codepoints are encoded as-is, the way PHP does.
                encode_utf8(codepoint, &mut out);
            }
            _ => {
                // Unknown escape: keep the backslash and the whole character.
                out.push(b'\\');
                out.push(escape);
                i += 1;
            }
        }
    }

    Ok(out)
}

fn simple(out: &mut Vec<u8>, byte: u8, i: &mut usize) {
    out.push(byte);
    *i += 1;
}

fn hex_value(byte: u8) -> u32 {
    match byte {
        b'0'..=b'9' => u32::from(byte - b'0'),
        b'a'..=b'f' => u32::from(byte - b'a') + 10,
        _ => u32::from(byte - b'A') + 10,
    }
}

fn ends_in_newline(bytes: &[u8]) -> bool {
    matches!(bytes.last(), Some(b'\r' | b'\n'))
}

/// Encodes a codepoint as UTF-8, including surrogate codepoints (PHP encodes
/// `"\u{D800}"` as the three bytes `ED A0 80`).
fn encode_utf8(codepoint: u32, out: &mut Vec<u8>) {
    if codepoint < 0x80 {
        out.push(codepoint as u8);
    } else if codepoint < 0x800 {
        out.push(0xC0 | (codepoint >> 6) as u8);
        out.push(0x80 | (codepoint & 0x3F) as u8);
    } else if codepoint < 0x10000 {
        out.push(0xE0 | (codepoint >> 12) as u8);
        out.push(0x80 | ((codepoint >> 6) & 0x3F) as u8);
        out.push(0x80 | (codepoint & 0x3F) as u8);
    } else {
        out.push(0xF0 | (codepoint >> 18) as u8);
        out.push(0x80 | ((codepoint >> 12) & 0x3F) as u8);
        out.push(0x80 | ((codepoint >> 6) & 0x3F) as u8);
        out.push(0x80 | (codepoint & 0x3F) as u8);
    }
}

/// A `NumString` offset in a string interpolation: an int key when it is a
/// canonical decimal integer, otherwise a string key.
fn num_string_literal(text: &str, span: Span) -> Expression {
    match canonical_decimal(text) {
        Some(value) => Expression {
            kind: ExpressionKind::Literal(Literal::Int(value)),
            span,
        },
        None => Expression {
            kind: ExpressionKind::Literal(Literal::String(text.as_bytes().to_vec())),
            span,
        },
    }
}

fn canonical_decimal(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return None;
    }
    text.parse::<i64>().ok()
}
