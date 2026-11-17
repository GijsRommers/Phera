//! `token_get_all("<?php /** comment */ echo 42;")` preserves trivia and spans.

use TokenKind::*;
use phera_lexer::{Lexer, TokenKind};

fn check(source: &[u8], expected: &[(TokenKind, &[u8])]) {
    let mut lexer = Lexer::for_tokenizer(source);
    let mut offset = 0;
    for &(kind, text) in expected {
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, kind);
        assert_eq!(token.span.start, offset);
        assert_eq!(&source[token.span.start..token.span.end], text);
        offset = token.span.end;
    }
    assert_eq!(offset, source.len());
    assert_eq!(lexer.next_token().unwrap().kind, Eof);
}

#[test]
fn trivia_and_binary_source() {
    check(
        b"\xff<?php\r\n\t// hi\r/** doc */ \0$\xff?>\r\n\xfex",
        &[
            (InlineHtml, b"\xff"),
            (OpenTag, b"<?php\r\n"),
            (Whitespace, b"\t"),
            (Comment, b"// hi"),
            (Whitespace, b"\r"),
            (DocComment, b"/** doc */"),
            (Whitespace, b" "),
            (BadCharacter, b"\0"),
            (Variable, b"$\xff"),
            (CloseTag, b"?>\r\n"),
            (InlineHtml, b"\xfex"),
        ],
    );
}

#[test]
fn keywords_after_member_access_and_ampersand_lookahead() {
    check(
        b"<?php $o->/* c */enum\\Name &\r\n$x & 1;",
        &[
            (OpenTag, b"<?php "),
            (Variable, b"$o"),
            (Arrow, b"->"),
            (Comment, b"/* c */"),
            (Identifier, b"enum"),
            (FullyQualifiedName, b"\\Name"),
            (Whitespace, b" "),
            (AmpersandBeforeVariable, b"&"),
            (Whitespace, b"\r\n"),
            (Variable, b"$x"),
            (Whitespace, b" "),
            (Ampersand, b"&"),
            (Whitespace, b" "),
            (Integer, b"1"),
            (Semicolon, b";"),
        ],
    );
}

#[test]
fn malformed_and_deprecated_source_still_has_tokens() {
    check(
        b"<?php `echo ${value}`; (real) 0; \"unterminated",
        &[
            (OpenTag, b"<?php "),
            (Backquote, b"`"),
            (StringPart, b"echo "),
            (DollarCurlyOpen, b"${"),
            (StringVarname, b"value"),
            (RightBrace, b"}"),
            (Backquote, b"`"),
            (Semicolon, b";"),
            (Whitespace, b" "),
            (FloatCast, b"(real)"),
            (Whitespace, b" "),
            (Integer, b"0"),
            (Semicolon, b";"),
            (Whitespace, b" "),
            (DoubleQuote, b"\""),
            (StringPart, b"unterminated"),
        ],
    );
    check(
        b"<?php /** unterminated",
        &[(OpenTag, b"<?php "), (DocComment, b"/** unterminated")],
    );
}

#[test]
fn halt_counts_inline_html_and_preserves_the_remaining_bytes() {
    check(
        b"<?php __halt_compiler?>x<?php y;?>z",
        &[
            (OpenTag, b"<?php "),
            (HaltCompiler, b"__halt_compiler"),
            (CloseTag, b"?>"),
            (InlineHtml, b"x"),
            (OpenTag, b"<?php "),
            (Identifier, b"y"),
            (InlineHtml, b";?>z"),
        ],
    );
}

#[test]
fn short_open_tags_follow_the_scanner_setting() {
    let code = b"prefix<? a;";
    let mut lexer = Lexer::for_tokenizer(code).with_short_open_tags(true);
    assert_eq!(lexer.next_token().unwrap().span.end, 6);
    let open = lexer.next_token().unwrap();
    assert_eq!(open.kind, OpenTag);
    assert_eq!(open.span.end, 8);
}
