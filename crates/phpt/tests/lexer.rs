//! Compares Phera's lexer with PHP's own tokenizer (`token_get_all`) on the
//! `--FILE--` section of each selected `.phpt` test.
//!
//! Only tests the lexer is meant to handle are compared: a test is "not
//! relevant" if PHP produces any token Phera doesn't support yet (comments,
//! interpolated strings, casts, ...). Those are counted, not failed.
//!
//! Needs a `php` binary (override with `PHP=/path/to/php`); skipped without one.
//! Set `PHERA_LEX_SCAN=<dir>` to check every `.phpt` file under a directory,
//! such as a php-src checkout, instead of the selected tests.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use phera_lexer::{Lexer, TokenKind, keyword};
use phera_phpt::{Phpt, display_path, filters_from_args, matches_filters};

fn main() -> ExitCode {
    let php = std::env::var("PHP").unwrap_or_else(|_| "php".into());

    if Command::new(&php).arg("-v").output().is_err() {
        println!("skipping lexer comparison: `{php}` not found (set PHP=/path/to/php)");
        return ExitCode::SUCCESS;
    }

    let scan_dir = std::env::var_os("PHERA_LEX_SCAN")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from);
    let tests = match &scan_dir {
        Some(dir) => phera_phpt::find_phpt_files(dir),
        None => match phera_phpt::selected_tests() {
            Ok(tests) => tests,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::FAILURE;
            }
        },
    };

    let filters = filters_from_args();
    let tests: Vec<_> = tests
        .into_iter()
        .filter(|t| matches_filters(t, &filters))
        .collect();

    let name = |path: &Path| match &scan_dir {
        Some(dir) => path.strip_prefix(dir).unwrap_or(path).display().to_string(),
        None => display_path(path),
    };

    // Write every `--FILE--` section out so PHP can tokenize them in one process.
    let work_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("phpt-lexer");
    let _ = fs::remove_dir_all(&work_dir);
    fs::create_dir_all(&work_dir).expect("cannot create work directory");

    let mut sources = Vec::new();

    for (index, path) in tests.iter().enumerate() {
        let Ok(phpt) = Phpt::parse_file(path) else {
            continue;
        };
        let Some(code) = phpt.section("FILE") else {
            continue;
        };

        let file = work_dir.join(format!("{index}.php"));
        fs::write(&file, code).expect("cannot write source file");
        sources.push((path, file, code.to_string()));
    }

    let php_tokens =
        match tokenize_with_php(&php, sources.iter().map(|(_, file, _)| file.as_path())) {
            Ok(tokens) => tokens,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::FAILURE;
            }
        };

    let mut passed = 0;
    let mut failures = Vec::new();
    let mut not_relevant: BTreeMap<String, usize> = BTreeMap::new();

    for ((path, _, code), tokens) in sources.iter().zip(php_tokens) {
        let expected = match expected_tokens(&tokens) {
            Ok(expected) => expected,
            Err(reason) => {
                *not_relevant.entry(reason).or_default() += 1;
                continue;
            }
        };

        match compare(code, &expected) {
            Ok(()) => {
                passed += 1;
                println!("PASS  {}", name(path));
            }
            Err(details) => {
                println!("FAIL  {}", name(path));
                failures.push((name(path), details));
            }
        }
    }

    for (name, details) in &failures {
        println!("\n---- {name} ----\n{details}");
    }

    let skipped: usize = not_relevant.values().sum();

    if skipped > 0 {
        println!("\nnot relevant (first unsupported PHP token, most common first):");

        let mut reasons: Vec<_> = not_relevant.into_iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        for (reason, count) in reasons.iter().take(15) {
            println!("  {count:>6}  {reason}");
        }
        if reasons.len() > 15 {
            println!("  ... and {} more kinds", reasons.len() - 15);
        }
    }

    println!(
        "\nlexer result: {passed} passed; {} failed; {skipped} not relevant; {} total",
        failures.len(),
        sources.len()
    );

    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// A token from PHP's tokenizer.
struct PhpToken {
    name: String,
    text: String,
}

fn tokenize_with_php<'a>(
    php: &str,
    files: impl Iterator<Item = &'a Path>,
) -> Result<Vec<Vec<PhpToken>>, String> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tokenize.php");

    let mut child = Command::new(php)
        .arg("-n")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start `{php}`: {e}"))?;

    let mut input = String::new();
    for file in files {
        input.push_str(&file.to_string_lossy());
        input.push('\n');
    }

    let mut stdin = child.stdin.take().expect("stdin is piped");
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));

    let output = child
        .wait_with_output()
        .map_err(|e| format!("cannot run `{php}`: {e}"))?;
    let _ = writer.join();

    if !output.status.success() {
        return Err(format!("`{php}` failed with {}", output.status));
    }

    let mut files = Vec::new();

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.starts_with("@@ ") {
            files.push(Vec::new());
            continue;
        }

        let (name, hex) = line.rsplit_once(' ').ok_or("malformed tokenizer output")?;
        let token = PhpToken {
            name: name.to_string(),
            text: decode_hex(hex)?,
        };

        files
            .last_mut()
            .ok_or("malformed tokenizer output")?
            .push(token);
    }

    Ok(files)
}

fn decode_hex(hex: &str) -> Result<String, String> {
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("bad hex in tokenizer output: {e}"))?;

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Maps PHP's tokens to what Phera's lexer should produce, skipping whitespace
/// and comments. Fails with the first PHP token Phera doesn't support.
fn expected_tokens(tokens: &[PhpToken]) -> Result<Vec<(TokenKind, &str)>, String> {
    let mut expected = Vec::new();

    for token in tokens {
        let text = token.text.as_str();

        if matches!(
            token.name.as_str(),
            "T_WHITESPACE" | "T_COMMENT" | "T_DOC_COMMENT"
        ) {
            continue;
        }

        // Phera leaves casts like `(int)` to the parser: `(`, `int`, `)`.
        if token.name.ends_with("_CAST") {
            let word = text[1..text.len() - 1].trim_matches([' ', '\t']);
            expected.extend([
                (TokenKind::LeftParen, "("),
                (name_kind(word), word),
                (TokenKind::RightParen, ")"),
            ]);
            continue;
        }

        // Phera leaves `yield from` to the parser too.
        if token.name == "T_YIELD_FROM" {
            expected.extend(text.split_whitespace().map(|word| (name_kind(word), word)));
            continue;
        }

        let kind = match token.name.as_str() {
            // Phera doesn't stop at `__halt_compiler`, or support short `<?` tags.
            "T_HALT_COMPILER" => None,
            "T_OPEN_TAG" if !text.to_ascii_lowercase().starts_with("<?php") => None,
            "T_INLINE_HTML" => Some(TokenKind::InlineHtml),

            // Keywords, and names PHP treats as plain strings in context, like
            // `$a->class`: Phera always lexes names as keywords where possible.
            _ if is_name(text) => Some(name_kind(text)),

            "T_OPEN_TAG" => Some(TokenKind::OpenTag),
            "T_OPEN_TAG_WITH_ECHO" => Some(TokenKind::OpenTagWithEcho),
            "T_CLOSE_TAG" => Some(TokenKind::CloseTag),
            "T_VARIABLE" => Some(TokenKind::Variable),
            "T_NAME_QUALIFIED" => Some(TokenKind::QualifiedName),
            "T_NAME_FULLY_QUALIFIED" => Some(TokenKind::FullyQualifiedName),
            "T_NAME_RELATIVE" => Some(TokenKind::RelativeName),
            "T_LNUMBER" => Some(TokenKind::Integer),
            // Integers too big for 64 bits are floats in PHP, but not in Phera's lexer.
            "T_DNUMBER" if !looks_like_float(text) => Some(TokenKind::Integer),
            "T_DNUMBER" => Some(TokenKind::Float),
            "T_CONSTANT_ENCAPSED_STRING" if text.starts_with(['\'', '"']) => {
                Some(TokenKind::String)
            }
            name => operator_kind(name),
        };

        let Some(kind) = kind else {
            return Err(format!("{} {text:?}", token.name));
        };

        // PHP includes the whitespace after `<?php` in the tag; Phera doesn't.
        let text = if kind == TokenKind::OpenTag {
            text.trim_end()
        } else {
            text
        };

        expected.push((kind, text));
    }

    Ok(expected)
}

fn is_name(text: &str) -> bool {
    let is_start = |c: char| c.is_ascii_alphabetic() || c == '_' || !c.is_ascii();
    let mut chars = text.chars();

    chars.next().is_some_and(is_start) && chars.all(|c| is_start(c) || c.is_ascii_digit())
}

fn name_kind(name: &str) -> TokenKind {
    keyword(&name.to_ascii_lowercase()).unwrap_or(TokenKind::Identifier)
}

fn looks_like_float(text: &str) -> bool {
    let prefixed =
        text.len() > 1 && text.starts_with('0') && text.as_bytes()[1].is_ascii_alphabetic();
    !prefixed && text.contains(['.', 'e', 'E'])
}

/// Maps PHP's operator and punctuation token names to Phera's.
fn operator_kind(name: &str) -> Option<TokenKind> {
    use TokenKind::*;

    let kind = match name {
        "T_INC" => Increment,
        "T_DEC" => Decrement,
        "T_POW" => Power,
        "T_PLUS_EQUAL" => PlusAssign,
        "T_MINUS_EQUAL" => MinusAssign,
        "T_MUL_EQUAL" => StarAssign,
        "T_DIV_EQUAL" => SlashAssign,
        "T_MOD_EQUAL" => PercentAssign,
        "T_POW_EQUAL" => PowerAssign,
        "T_CONCAT_EQUAL" => DotAssign,
        "T_COALESCE_EQUAL" => NullCoalesceAssign,
        "T_AND_EQUAL" => AmpersandAssign,
        "T_OR_EQUAL" => PipeAssign,
        "T_XOR_EQUAL" => CaretAssign,
        "T_SL_EQUAL" => ShiftLeftAssign,
        "T_SR_EQUAL" => ShiftRightAssign,
        "T_IS_EQUAL" => Equal,
        "T_IS_IDENTICAL" => Identical,
        "T_IS_NOT_EQUAL" => NotEqual,
        "T_IS_NOT_IDENTICAL" => NotIdentical,
        "T_IS_SMALLER_OR_EQUAL" => LessEqual,
        "T_IS_GREATER_OR_EQUAL" => GreaterEqual,
        "T_SPACESHIP" => Spaceship,
        "T_BOOLEAN_AND" => LogicalAnd,
        "T_BOOLEAN_OR" => LogicalOr,
        "T_SL" => ShiftLeft,
        "T_SR" => ShiftRight,
        "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG" | "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG" => {
            Ampersand
        }
        "T_OBJECT_OPERATOR" => Arrow,
        "T_NULLSAFE_OBJECT_OPERATOR" => NullsafeArrow,
        "T_DOUBLE_COLON" | "T_PAAMAYIM_NEKUDOTAYIM" => DoubleColon,
        "T_DOUBLE_ARROW" => FatArrow,
        "T_COALESCE" => NullCoalesce,
        "T_PIPE" => PipeForward,
        "T_ELLIPSIS" => Ellipsis,
        "T_ATTRIBUTE" => AttributeStart,
        "T_NS_SEPARATOR" => Backslash,
        "+" => Plus,
        "-" => Minus,
        "*" => Star,
        "/" => Slash,
        "%" => Percent,
        "." => Dot,
        "=" => Assign,
        "!" => Not,
        "<" => Less,
        ">" => Greater,
        "|" => Pipe,
        "^" => Caret,
        "~" => Tilde,
        "?" => Question,
        "@" => At,
        "$" => Dollar,
        ":" => Colon,
        ";" => Semicolon,
        "," => Comma,
        "(" => LeftParen,
        ")" => RightParen,
        "{" => LeftBrace,
        "}" => RightBrace,
        "[" => LeftBracket,
        "]" => RightBracket,
        _ => return None,
    };

    Some(kind)
}

/// Lexes `code` with Phera and compares it with PHP's tokens.
fn compare(code: &str, expected: &[(TokenKind, &str)]) -> Result<(), String> {
    let mut lexer = Lexer::new(code);

    for (index, &(kind, text)) in expected.iter().enumerate() {
        let token = lexer.next_token().map_err(|error| {
            format!("token {index}: PHP has {kind:?} {text:?}, Phera errors with {error:?}")
        })?;

        let actual = &code[token.span.start..token.span.end];

        if token.kind != kind || actual != text {
            return Err(format!(
                "token {index}: PHP has {kind:?} {text:?}, Phera has {:?} {actual:?}\n  at byte {} in:\n{}",
                token.kind,
                token.span.start,
                context(code, token.span.start)
            ));
        }
    }

    match lexer.next_token() {
        Ok(token) if token.kind == TokenKind::Eof => Ok(()),
        Ok(token) => Err(format!(
            "Phera has an extra {:?} token at byte {} after PHP's last token",
            token.kind, token.span.start
        )),
        Err(error) => Err(format!("Phera errors after PHP's last token: {error:?}")),
    }
}

/// The line of `code` containing `offset`, with a caret under it.
fn context(code: &str, offset: usize) -> String {
    let start = code[..offset].rfind('\n').map_or(0, |i| i + 1);
    let end = code[offset..].find('\n').map_or(code.len(), |i| offset + i);
    let column = code[start..offset].chars().count();

    format!("    {}\n    {}^", &code[start..end], " ".repeat(column))
}
