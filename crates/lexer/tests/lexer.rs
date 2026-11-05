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
fn binary_string_prefix() {
    assert_eq!(
        lexemes(r#"b'a' B"b" b"c $d""#),
        ["b'a'", r#"B"b""#, r#"b""#, "c ", "$d", "\""]
    );
    assert_eq!(kinds("b'a' bb'a'"), [String, Identifier, String]);
}

// --- Interpolated strings -----------------------------------------------------

#[test]
fn interpolated_variable() {
    let source = r#""hi $name!""#;
    assert_eq!(
        kinds(source),
        [DoubleQuote, StringPart, Variable, StringPart, DoubleQuote]
    );
    assert_eq!(lexemes(source), ["\"", "hi ", "$name", "!", "\""]);
}

#[test]
fn interpolation_at_the_edges() {
    assert_eq!(kinds(r#""$a""#), [DoubleQuote, Variable, DoubleQuote]);
    assert_eq!(
        kinds(r#""$a$b""#),
        [DoubleQuote, Variable, Variable, DoubleQuote]
    );
}

#[test]
fn dollar_without_a_name_is_literal() {
    assert_eq!(kinds(r#""$ $1 \$a""#), [String]);
    assert_eq!(
        kinds(r#""$ $b""#),
        [DoubleQuote, StringPart, Variable, DoubleQuote]
    );
}

#[test]
fn escaped_dollar_is_literal_but_escaped_brace_is_not() {
    assert_eq!(kinds(r#""\$a \{\$b}""#), [String]);
    // `\{` only escapes the brace: PHP still interpolates the `$a` after it.
    assert_eq!(lexemes(r#""\{$a}""#), ["\"", r"\{", "$a", "}", "\""]);
}

#[test]
fn interpolated_array_offsets() {
    assert_eq!(
        lexemes(r#""$a[0] $a[key] $a[$i] $a[-1] $a[0x1F]""#),
        [
            "\"", "$a", "[", "0", "]", " ", "$a", "[", "key", "]", " ", "$a", "[", "$i", "]", " ",
            "$a", "[", "-", "1", "]", " ", "$a", "[", "0x1F", "]", "\"",
        ]
    );
    assert_eq!(
        kinds(r#""$a[0]$b[k]""#),
        [
            DoubleQuote,
            Variable,
            LeftBracket,
            NumString,
            RightBracket,
            Variable,
            LeftBracket,
            Identifier,
            RightBracket,
            DoubleQuote,
        ]
    );
}

#[test]
fn only_one_offset_is_interpolated() {
    assert_eq!(
        lexemes(r#""$a[0][1]""#),
        ["\"", "$a", "[", "0", "]", "[1]", "\""]
    );
}

#[test]
fn interpolated_properties() {
    assert_eq!(
        kinds(r#""$a->b $a?->b""#),
        [
            DoubleQuote,
            Variable,
            Arrow,
            Identifier,
            StringPart,
            Variable,
            NullsafeArrow,
            Identifier,
            DoubleQuote,
        ]
    );
    // Only one level of property access, and only when a name follows.
    assert_eq!(
        lexemes(r#""$a->b->c $a-> $a->1""#),
        [
            "\"", "$a", "->", "b", "->c ", "$a", "-> ", "$a", "->1", "\""
        ]
    );
}

#[test]
fn curly_interpolation() {
    assert_eq!(
        kinds(r#""{$a} {$a->b()} {$a['k']}""#),
        [
            DoubleQuote,
            CurlyOpen,
            Variable,
            RightBrace,
            StringPart,
            CurlyOpen,
            Variable,
            Arrow,
            Identifier,
            LeftParen,
            RightParen,
            RightBrace,
            StringPart,
            CurlyOpen,
            Variable,
            LeftBracket,
            String,
            RightBracket,
            RightBrace,
            DoubleQuote,
        ]
    );
    assert_eq!(lexemes(r#""{$a}""#), ["\"", "{", "$a", "}", "\""]);
}

#[test]
fn braces_without_a_dollar_are_literal() {
    assert_eq!(
        kinds(r#""{ $a } {}""#),
        [DoubleQuote, StringPart, Variable, StringPart, DoubleQuote]
    );
    assert_eq!(
        lexemes(r#""{{$a}}""#),
        ["\"", "{", "{", "$a", "}", "}", "\""]
    );
}

#[test]
fn nested_strings_inside_curly_interpolation() {
    assert_eq!(
        kinds(r#""a {$f("x $y")} b""#),
        [
            DoubleQuote,
            StringPart,
            CurlyOpen,
            Variable,
            LeftParen,
            DoubleQuote,
            StringPart,
            Variable,
            DoubleQuote,
            RightParen,
            RightBrace,
            StringPart,
            DoubleQuote,
        ]
    );
}

#[test]
fn unterminated_interpolated_strings() {
    for source in [r#""$a"#, r#""$a "#, r#""{$a"#, r#""{$a}"#] {
        assert_eq!(
            first_error(source).kind,
            LexErrorKind::UnterminatedString,
            "in {source:?}"
        );
    }
}

#[test]
fn deprecated_dollar_brace_interpolation_is_rejected() {
    for source in [r#""${a}""#, r#""${a[1]}""#, r#""${expr()}""#] {
        assert!(
            matches!(first_error(source).kind, LexErrorKind::Unsupported(_)),
            "in {source:?}"
        );
    }
}

#[test]
fn invalid_offsets_are_errors() {
    assert_eq!(
        first_error(r#""$a[ 0]""#).kind,
        LexErrorKind::UnexpectedCharacter(' ')
    );
    assert_eq!(
        first_error(r#""$a['k']""#).kind,
        LexErrorKind::UnexpectedCharacter('\'')
    );
}

// --- Heredocs and nowdocs ------------------------------------------------------

#[test]
fn heredoc() {
    let source = "<<<EOT\nhello $name\nbye\nEOT;";
    assert_eq!(
        kinds(source),
        [
            StartHeredoc,
            StringPart,
            Variable,
            StringPart,
            EndHeredoc,
            Semicolon
        ]
    );
    assert_eq!(
        lexemes(source),
        ["<<<EOT\n", "hello ", "$name", "\nbye\n", "EOT", ";"]
    );
}

#[test]
fn heredoc_label_styles() {
    assert_eq!(lexemes("<<<\"EOT\"\nx\nEOT;")[0], "<<<\"EOT\"\n");
    assert_eq!(lexemes("<<< EOT\nx\nEOT;")[0], "<<< EOT\n");
    assert_eq!(lexemes("b<<<EOT\nx\nEOT;")[0], "b<<<EOT\n");
    assert_eq!(
        lexemes("<<<EOT\r\nx\r\nEOT\r\n"),
        ["<<<EOT\r\n", "x\r\n", "EOT"]
    );
}

#[test]
fn nowdoc_is_one_literal_part() {
    let source = "<<<'EOT'\n  $x {$y} \\n\n  EOT;";
    assert_eq!(
        kinds(source),
        [StartHeredoc, StringPart, EndHeredoc, Semicolon]
    );
    assert_eq!(lexemes(source)[1], "  $x {$y} \\n\n");
}

#[test]
fn heredoc_closing_label_keeps_its_indentation() {
    assert_eq!(
        lexemes("<<<EOT\n    a\n    EOT;"),
        ["<<<EOT\n", "    a\n", "    EOT", ";"]
    );
    assert_eq!(
        lexemes("<<<EOT\n\ta\n\tEOT\n"),
        ["<<<EOT\n", "\ta\n", "\tEOT"]
    );
}

#[test]
fn empty_heredoc() {
    assert_eq!(kinds("<<<EOT\nEOT;"), [StartHeredoc, EndHeredoc, Semicolon]);
    assert_eq!(kinds("<<<EOT\n  EOT\n"), [StartHeredoc, EndHeredoc]);
    assert_eq!(lexemes("<<<EOT\n\nEOT\n"), ["<<<EOT\n", "\n", "EOT"]);
}

#[test]
fn heredoc_label_must_be_a_whole_word() {
    assert_eq!(
        lexemes("<<<EOT\nEOTX\nEOT, 1"),
        ["<<<EOT\n", "EOTX\n", "EOT", ",", "1"]
    );
}

#[test]
fn heredoc_interpolation() {
    assert_eq!(
        lexemes("<<<EOT\n$a[0]$b->c{$d}\nEOT;"),
        [
            "<<<EOT\n", "$a", "[", "0", "]", "$b", "->", "c", "{", "$d", "}", "\n", "EOT", ";"
        ]
    );
}

#[test]
fn heredoc_escapes() {
    assert_eq!(
        lexemes("<<<EOT\n\\$a \\\\\nEOT\n"),
        ["<<<EOT\n", "\\$a \\\\\n", "EOT"]
    );
}

#[test]
fn backslash_before_a_newline_does_not_hide_the_closing_label() {
    assert_eq!(
        lexemes("<<<EOT\na\\\nEOT;"),
        ["<<<EOT\n", "a\\\n", "EOT", ";"]
    );
}

#[test]
fn unterminated_heredocs() {
    for source in ["<<<EOT\nno end", "<<<EOT\nEOT", "<<<'EOT'\nx\n", "<<<EOT\n"] {
        assert_eq!(
            first_error(source).kind,
            LexErrorKind::UnterminatedHeredoc,
            "in {source:?}"
        );
    }
}

#[test]
fn heredoc_start_must_be_followed_by_a_newline() {
    assert_eq!(kinds("<<<EOT x"), [ShiftLeft, Less, Identifier, Identifier]);
    assert_eq!(kinds("<<<'EOT' x"), [ShiftLeft, Less, String, Identifier]);
}

#[test]
fn backticks_are_rejected() {
    assert!(matches!(
        first_error("`ls`").kind,
        LexErrorKind::Unsupported(_)
    ));
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

// --- Context-dependent keywords ------------------------------------------------

#[test]
fn casts() {
    assert_eq!(
        kinds(
            "(int) (integer) (float) (double) (string) (binary) (array) (object) (bool) (boolean) (unset) (void)"
        ),
        [
            IntCast, IntCast, FloatCast, FloatCast, StringCast, StringCast, ArrayCast, ObjectCast,
            BoolCast, BoolCast, UnsetCast, VoidCast,
        ]
    );
    assert_eq!(lexemes("( INT ) (\tstring\t)"), ["( INT )", "(\tstring\t)"]);
}

#[test]
fn parens_that_are_not_casts() {
    assert_eq!(kinds("(int"), [LeftParen, Identifier]);
    assert_eq!(kinds("(ints)"), [LeftParen, Identifier, RightParen]);
    assert_eq!(kinds("(\nint)"), [LeftParen, Identifier, RightParen]);
    assert_eq!(kinds("(int)x"), [IntCast, Identifier]);
}

#[test]
fn real_cast_is_rejected() {
    assert!(matches!(
        first_error("(real) 1").kind,
        LexErrorKind::Unsupported(_)
    ));
}

#[test]
fn magic_constants() {
    assert_eq!(
        kinds(
            "__DIR__ __FILE__ __LINE__ __CLASS__ __FUNCTION__ __METHOD__ __NAMESPACE__ __TRAIT__ __PROPERTY__ __dir__"
        ),
        [
            MagicDir,
            MagicFile,
            MagicLine,
            MagicClass,
            MagicFunction,
            MagicMethod,
            MagicNamespace,
            MagicTrait,
            MagicProperty,
            MagicDir,
        ]
    );
}

#[test]
fn enum_is_only_a_keyword_before_a_name() {
    assert_eq!(kinds("enum Suit"), [Enum, Identifier]);
    assert_eq!(kinds("Enum\n/* c */ Suit"), [Enum, Identifier]);
    assert_eq!(kinds("enum extends"), [Identifier, Extends]);
    assert_eq!(kinds("enum implements"), [Identifier, Implements]);
    assert_eq!(
        kinds("enum(1)"),
        [Identifier, LeftParen, Integer, RightParen]
    );
    assert_eq!(kinds("enum"), [Identifier]);
}

#[test]
fn yield_from() {
    assert_eq!(lexemes("yield from $g"), ["yield from", "$g"]);
    assert_eq!(
        lexemes("yield\n  /* x */ from $g"),
        ["yield\n  /* x */ from", "$g"]
    );
    assert_eq!(kinds("YIELD FROM $g"), [YieldFrom, Variable]);
    assert_eq!(kinds("yield fromage"), [Yield, Identifier]);
    assert_eq!(kinds("yield $x"), [Yield, Variable]);
}

#[test]
fn asymmetric_visibility() {
    assert_eq!(
        kinds("public(set) protected(set) PRIVATE(SET)"),
        [PublicSet, ProtectedSet, PrivateSet]
    );
    assert_eq!(
        kinds("private (set) private(get)"),
        [
            Private, LeftParen, Identifier, RightParen, Private, LeftParen, Identifier, RightParen
        ]
    );
}

#[test]
fn halt_compiler_turns_the_rest_into_data() {
    assert_eq!(
        lexemes("__halt_compiler(); raw <?php $x"),
        ["__halt_compiler", "(", ")", ";", " raw <?php $x"]
    );
    assert_eq!(
        kinds("__HALT_COMPILER ( ) ?> raw"),
        [HaltCompiler, LeftParen, RightParen, CloseTag, InlineHtml]
    );
    assert_eq!(
        kinds("__halt_compiler();"),
        [HaltCompiler, LeftParen, RightParen, Semicolon]
    );
}

#[test]
fn halt_compiler_as_a_member_name_does_not_halt() {
    assert_eq!(
        kinds("$o->__halt_compiler(); $x"),
        [
            Variable,
            Arrow,
            HaltCompiler,
            LeftParen,
            RightParen,
            Semicolon,
            Variable
        ]
    );
}

#[test]
fn braces_nest_without_affecting_code() {
    assert_eq!(
        kinds("{ { } } }"),
        [LeftBrace, LeftBrace, RightBrace, RightBrace, RightBrace]
    );
    assert_eq!(
        file_kinds("<?php { ?>x<?php }"),
        [
            OpenTag, LeftBrace, CloseTag, InlineHtml, OpenTag, RightBrace
        ]
    );
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
    for (source, c) in [("\u{1}", '\u{1}'), ("\u{c}", '\u{c}')] {
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
    let error = first_error("$a = \u{1};");
    assert_eq!(error.kind, LexErrorKind::UnexpectedCharacter('\u{1}'));
    assert_eq!(error.span, Span::new(5, 6));
}

#[test]
fn underscore_must_follow_a_digit() {
    assert_eq!(lexemes("100._0"), ["100.", "_0"]);
}
