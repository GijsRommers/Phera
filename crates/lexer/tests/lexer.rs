use phera_lexer::{LexError, LexErrorKind, Lexer, Token, TokenKind};
use phera_source::Span;

use TokenKind::*;

/// Lexes a whole file to completion, returning every token including `Eof`.
fn file_tokens(source: &str) -> Vec<Token> {
    collect(source, Lexer::new(source))
}

/// Lexes PHP code (no `<?php` tag needed), returning every token including `Eof`.
fn tokens(source: &str) -> Vec<Token> {
    collect(source, Lexer::new_php(source))
}

fn collect(source: &str, mut lexer: Lexer) -> Vec<Token> {
    let mut tokens = Vec::new();

    loop {
        let token = lexer
            .next_token()
            .unwrap_or_else(|e| panic!("unexpected lex error in {source:?}: {e:?}"));
        let done = token.kind == Eof;
        tokens.push(token);

        if done {
            return tokens;
        }
    }
}

/// The token kinds in `tokens`, without the final `Eof`.
fn kinds_of(tokens: Vec<Token>) -> Vec<TokenKind> {
    let mut kinds: Vec<_> = tokens.into_iter().map(|t| t.kind).collect();
    assert_eq!(kinds.pop(), Some(Eof));
    kinds
}

/// The token kinds of PHP code, without the final `Eof`.
fn kinds(source: &str) -> Vec<TokenKind> {
    kinds_of(tokens(source))
}

/// The token kinds of a whole file, without the final `Eof`.
fn file_kinds(source: &str) -> Vec<TokenKind> {
    kinds_of(file_tokens(source))
}

/// The source text of each token in PHP code, without the final `Eof`.
fn lexemes(source: &str) -> Vec<&str> {
    tokens(source)
        .iter()
        .filter(|t| t.kind != Eof)
        .map(|t| &source[t.span.start..t.span.end])
        .collect()
}

/// Lexes PHP code until the first error and returns it.
fn first_error(source: &str) -> LexError {
    let mut lexer = Lexer::new_php(source);

    loop {
        match lexer.next_token() {
            Ok(token) if token.kind == Eof => panic!("expected a lex error in {source:?}"),
            Ok(_) => {}
            Err(error) => return error,
        }
    }
}

// --- Files, tags and inline HTML --------------------------------------------

#[test]
fn empty_source_is_just_eof() {
    assert_eq!(
        file_tokens(""),
        [Token {
            kind: Eof,
            span: Span::new(0, 0)
        }]
    );
    assert_eq!(
        tokens(""),
        [Token {
            kind: Eof,
            span: Span::new(0, 0)
        }]
    );
}

#[test]
fn eof_is_returned_repeatedly() {
    let mut lexer = Lexer::new_php("1");
    assert_eq!(lexer.next_token().unwrap().kind, Integer);
    assert_eq!(lexer.next_token().unwrap().kind, Eof);
    assert_eq!(lexer.next_token().unwrap().kind, Eof);
}

#[test]
fn files_start_as_inline_html() {
    assert_eq!(file_kinds("just text"), [InlineHtml]);
    assert_eq!(
        file_kinds("<h1>Hi</h1><?php echo 1;"),
        [InlineHtml, OpenTag, Echo, Integer, Semicolon]
    );
}

#[test]
fn open_and_close_tags() {
    assert_eq!(file_kinds("<?php ?>"), [OpenTag, CloseTag]);
    assert_eq!(file_kinds("<?PHP\n?>"), [OpenTag, CloseTag]);
    assert_eq!(
        file_kinds("<?= $a ?>"),
        [OpenTagWithEcho, Variable, CloseTag]
    );
}

#[test]
fn php_tag_needs_whitespace_after_it() {
    assert_eq!(file_kinds("<?phpx"), [InlineHtml]);
    assert_eq!(file_kinds("<?php"), [OpenTag]);
}

#[test]
fn inline_html_between_tags() {
    let source = "<?php if ($a): ?>\n<p>yes</p>\n<?php endif; ?>";
    assert_eq!(
        file_kinds(source),
        [
            OpenTag, If, LeftParen, Variable, RightParen, Colon, CloseTag, InlineHtml, OpenTag,
            EndIf, Semicolon, CloseTag,
        ]
    );
}

#[test]
fn close_tag_swallows_one_newline() {
    let source = "<?php ?>\n\nhi";
    let tokens = file_tokens(source);
    let texts: Vec<_> = tokens
        .iter()
        .map(|t| &source[t.span.start..t.span.end])
        .collect();
    assert_eq!(texts, ["<?php", "?>\n", "\nhi", ""]);
}

// --- Comments ---------------------------------------------------------------

#[test]
fn comments_are_skipped() {
    assert_eq!(
        kinds("1 // line\n2 # hash\n3 /* block */ 4 /** doc */"),
        [Integer; 4]
    );
}

#[test]
fn line_comment_ends_at_close_tag() {
    assert_eq!(
        file_kinds("<?php // comment ?> html"),
        [OpenTag, CloseTag, InlineHtml]
    );
}

#[test]
fn hash_bracket_is_an_attribute_not_a_comment() {
    assert_eq!(
        kinds("#[Route]"),
        [AttributeStart, Identifier, RightBracket]
    );
}

#[test]
fn unterminated_block_comment() {
    let error = first_error("1 /* oops");
    assert_eq!(error.kind, LexErrorKind::UnterminatedComment);
    assert_eq!(error.span, Span::new(2, 9));
}

// --- Names, keywords and variables ------------------------------------------

#[test]
fn keywords() {
    assert_eq!(
        kinds("if else elseif function fn return echo class new foreach as match"),
        [
            If, Else, ElseIf, Function, Fn, Return, Echo, Class, New, Foreach, As, Match
        ]
    );
    assert_eq!(kinds("exit die"), [Exit, Exit]);
    assert_eq!(kinds("true false null"), [True, False, Null]);
}

#[test]
fn keywords_are_case_insensitive() {
    assert_eq!(kinds("IF Echo NULL True"), [If, Echo, Null, True]);
}

#[test]
fn identifiers() {
    assert_eq!(kinds("foo _bar Baz9 iff elsewhere"), [Identifier; 5]);
    assert_eq!(lexemes("foo _bar"), ["foo", "_bar"]);
}

#[test]
fn non_ascii_names() {
    assert_eq!(kinds("$café naïve"), [Variable, Identifier]);
}

#[test]
fn qualified_names() {
    assert_eq!(
        kinds(r"Foo\Bar \Foo\Bar namespace\Foo \Foo \ Foo"),
        [
            QualifiedName,
            FullyQualifiedName,
            RelativeName,
            FullyQualifiedName,
            Backslash,
            Identifier
        ]
    );
    assert_eq!(
        lexemes(r"use App\Http\Controller;"),
        ["use", r"App\Http\Controller", ";"]
    );
}

#[test]
fn variables() {
    assert_eq!(kinds("$a $foo_bar $x1 $this"), [Variable; 4]);
    assert_eq!(lexemes("$a $foo_bar"), ["$a", "$foo_bar"]);
}

#[test]
fn dollar_without_a_name() {
    assert_eq!(kinds("$$a"), [Dollar, Variable]);
    assert_eq!(kinds("${'a'}"), [Dollar, LeftBrace, String, RightBrace]);
    assert_eq!(kinds("$1"), [Dollar, Integer]);
}

// --- Numbers ----------------------------------------------------------------

#[test]
fn integers() {
    assert_eq!(kinds("0 42 0x1F 0b101 0o17 0777 1_000_000"), [Integer; 7]);
    assert_eq!(lexemes("0x1F 1_000"), ["0x1F", "1_000"]);
}

#[test]
fn floats() {
    assert_eq!(kinds("3.14 .5 1. 1e3 1E-3 1.5e+10 1_0.5"), [Float; 7]);
}

#[test]
fn number_edge_cases() {
    // A prefix or `_` without digits after it ends the number.
    assert_eq!(lexemes("0x 1_ 1e"), ["0", "x", "1", "_", "1", "e"]);
    // `1.` is a float, as in PHP.
    assert_eq!(kinds("1.$a"), [Float, Variable]);
    assert_eq!(kinds("1 . 2"), [Integer, Dot, Integer]);
}

// --- Strings ----------------------------------------------------------------

#[test]
fn strings() {
    assert_eq!(kinds(r#"'single' "double""#), [String, String]);
    assert_eq!(lexemes(r#"'a' "b""#), ["'a'", r#""b""#]);
}

#[test]
fn strings_with_escapes() {
    let source = r#"'it\'s' "say \"hi\"" "a \$b" 'a $b'"#;
    assert_eq!(kinds(source), [String; 4]);
    assert_eq!(lexemes(source)[0], r"'it\'s'");
}

#[test]
fn string_containing_other_quote_kind() {
    assert_eq!(lexemes(r#"'a"b' "c'd""#), [r#"'a"b'"#, r#""c'd""#]);
}

#[test]
fn string_with_multibyte_characters() {
    let source = "'héllo 🦀'";
    assert_eq!(lexemes(source), [source]);
}

#[test]
fn unterminated_strings() {
    let error = first_error("$a = 'oops");
    assert_eq!(error.kind, LexErrorKind::UnterminatedString);
    assert_eq!(error.span, Span::new(5, 10));

    assert_eq!(
        first_error(r#""abc\"#).kind,
        LexErrorKind::UnterminatedString
    );
}

#[test]
fn not_yet_supported_strings() {
    for source in [
        r#""hi $name""#,
        r#""hi {$a}""#,
        "<<<EOT\nhi\nEOT",
        "<<<'EOT'\nhi\nEOT",
    ] {
        assert!(
            matches!(first_error(source).kind, LexErrorKind::Unsupported(_)),
            "in {source:?}"
        );
    }
}

// --- Operators --------------------------------------------------------------

#[test]
fn arithmetic_operators() {
    assert_eq!(
        kinds("+ - * / % ** . ++ --"),
        [
            Plus, Minus, Star, Slash, Percent, Power, Dot, Increment, Decrement
        ]
    );
}

#[test]
fn assignment_operators() {
    assert_eq!(
        kinds("= += -= *= /= %= **= .= ??= &= |= ^= <<= >>="),
        [
            Assign,
            PlusAssign,
            MinusAssign,
            StarAssign,
            SlashAssign,
            PercentAssign,
            PowerAssign,
            DotAssign,
            NullCoalesceAssign,
            AmpersandAssign,
            PipeAssign,
            CaretAssign,
            ShiftLeftAssign,
            ShiftRightAssign,
        ]
    );
}

#[test]
fn comparison_operators() {
    assert_eq!(
        kinds("== === != <> !== < <= > >= <=>"),
        [
            Equal,
            Identical,
            NotEqual,
            NotEqual,
            NotIdentical,
            Less,
            LessEqual,
            Greater,
            GreaterEqual,
            Spaceship,
        ]
    );
}

#[test]
fn logical_and_bitwise_operators() {
    assert_eq!(
        kinds("! && || and or xor & | ^ ~ << >>"),
        [
            Not, LogicalAnd, LogicalOr, And, Or, Xor, Ampersand, Pipe, Caret, Tilde, ShiftLeft,
            ShiftRight
        ]
    );
}

#[test]
fn other_operators() {
    assert_eq!(
        kinds("-> ?-> => :: ?? ? : @ ... |>"),
        [
            Arrow,
            NullsafeArrow,
            FatArrow,
            DoubleColon,
            NullCoalesce,
            Question,
            Colon,
            At,
            Ellipsis,
            PipeForward
        ]
    );
}

#[test]
fn delimiters() {
    assert_eq!(
        kinds("( ) { } [ ] ; ,"),
        [
            LeftParen,
            RightParen,
            LeftBrace,
            RightBrace,
            LeftBracket,
            RightBracket,
            Semicolon,
            Comma
        ]
    );
}

#[test]
fn operators_without_whitespace() {
    assert_eq!(kinds("$a===$b"), [Variable, Identical, Variable]);
    assert_eq!(kinds("$a->b"), [Variable, Arrow, Identifier]);
    assert_eq!(kinds("A::b"), [Identifier, DoubleColon, Identifier]);
    assert_eq!(kinds("$a??$b"), [Variable, NullCoalesce, Variable]);
    assert_eq!(kinds("$a-1"), [Variable, Minus, Integer]);
    assert_eq!(kinds("$i++;"), [Variable, Increment, Semicolon]);
    assert_eq!(kinds("!$a"), [Not, Variable]);
}

// --- Whole programs, spans and errors ---------------------------------------

#[test]
fn small_program() {
    let source = r#"<?php
namespace App;

use Foo\Bar;

// Adds two numbers.
function add(int $a, int $b): int {
    return $a + $b;
}

echo add(1, 2.5) ?? 'none';
?>"#;

    assert_eq!(
        file_kinds(source),
        [
            OpenTag,
            Namespace,
            Identifier,
            Semicolon,
            Use,
            QualifiedName,
            Semicolon,
            Function,
            Identifier,
            LeftParen,
            Identifier,
            Variable,
            Comma,
            Identifier,
            Variable,
            RightParen,
            Colon,
            Identifier,
            LeftBrace,
            Return,
            Variable,
            Plus,
            Variable,
            Semicolon,
            RightBrace,
            Echo,
            Identifier,
            LeftParen,
            Integer,
            Comma,
            Float,
            RightParen,
            NullCoalesce,
            String,
            Semicolon,
            CloseTag,
        ]
    );
}

#[test]
fn spans_are_byte_offsets() {
    let spans: Vec<_> = tokens("$x = 'é';").iter().map(|t| t.span).collect();

    assert_eq!(
        spans,
        [
            Span::new(0, 2),   // $x
            Span::new(3, 4),   // =
            Span::new(5, 9),   // 'é' (é is 2 bytes)
            Span::new(9, 10),  // ;
            Span::new(10, 10), // Eof
        ]
    );
}

#[test]
fn unexpected_characters() {
    for (source, c) in [("\u{1}", '\u{1}'), ("`ls`", '`'), ("\u{c}", '\u{c}')] {
        let error = first_error(source);
        assert_eq!(
            error.kind,
            LexErrorKind::UnexpectedCharacter(c),
            "in {source:?}"
        );
        assert_eq!(error.span, Span::new(0, 1), "in {source:?}");
    }
}

#[test]
fn error_after_valid_tokens_reports_correct_span() {
    let error = first_error("$a = `b`;");
    assert_eq!(error.kind, LexErrorKind::UnexpectedCharacter('`'));
    assert_eq!(error.span, Span::new(5, 6));
}

#[test]
fn underscore_must_follow_a_digit() {
    assert_eq!(lexemes("100._0"), ["100.", "_0"]);
}
