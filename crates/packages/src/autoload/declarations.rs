//! Find named PHP declarations such as `namespace App; class Controller {}`.
//!
//! Composer scans declarations in source files without parsing their executable code.
//! Use Phera's PHP tokenizer so comments and strings cannot add false names.

use anyhow::Result;
use phera_lexer::{Lexer, Token, TokenKind};

pub(super) fn names(source: &[u8]) -> Result<Vec<String>> {
    use TokenKind::*;

    let mut lexer = Lexer::for_tokenizer(source);
    let mut namespace = "";
    let mut names = Vec::new();
    let mut previous = Eof;
    let mut current = next(&mut lexer)?;
    while current.kind != Eof {
        let following = next(&mut lexer)?;
        if !matches!(previous, Arrow | NullsafeArrow | DoubleColon) {
            match current.kind {
                Namespace => match following.kind {
                    Identifier | QualifiedName => namespace = text(source, &following)?,
                    LeftBrace => namespace = "",
                    _ => {}
                },
                Class | Interface | Trait | Enum if following.kind == Identifier => {
                    let name = text(source, &following)?;
                    names.push(if namespace.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{namespace}\\{name}")
                    });
                }
                _ => {}
            }
        }
        previous = current.kind;
        current = following;
    }
    Ok(names)
}

fn next(lexer: &mut Lexer<'_>) -> Result<Token> {
    loop {
        let token = lexer.next_token().map_err(|error| {
            anyhow::anyhow!(
                "Cannot tokenize PHP at byte {}: {:?}",
                error.span.start,
                error.kind
            )
        })?;
        if !matches!(
            token.kind,
            TokenKind::Whitespace | TokenKind::Comment | TokenKind::DocComment
        ) {
            return Ok(token);
        }
    }
}

fn text<'source>(source: &'source [u8], token: &Token) -> Result<&'source str> {
    Ok(std::str::from_utf8(
        &source[token.span.start..token.span.end],
    )?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn discovers_method_closure_and_anonymous_method_declarations() {
        // Observed with Composer ClassMapGenerator 1.9.0 under PHP 8.5.12.
        let source = "<?php namespace Acme; class OuterClass {\n\
            function factory() { class MethodNested {} } }\n\
            $factory = static function () { class ClosureNested {} };\n\
            $anonymous = new class { function factory() { class AnonymousMethodNested {} } };";
        assert_eq!(
            super::names(source.as_bytes()).unwrap(),
            [
                "Acme\\OuterClass",
                "Acme\\MethodNested",
                "Acme\\ClosureNested",
                "Acme\\AnonymousMethodNested"
            ]
        );
    }

    #[test]
    fn preserves_declarations_around_shell_expressions_and_legacy_interpolation() {
        let source = "<?php namespace Acme; class Runner {\n\
            function run($id) { $a = (int) $b = 1; echo \"${id}\"; \
            return `echo 'class Fake {}'`; }\n\
            } interface Contract {} trait Helper {} enum State {}";
        assert_eq!(
            super::names(source.as_bytes()).unwrap(),
            [
                "Acme\\Runner",
                "Acme\\Contract",
                "Acme\\Helper",
                "Acme\\State"
            ]
        );
    }

    #[test]
    fn scans_php_files_with_non_utf8_strings_and_incomplete_syntax() {
        let source = b"<?php namespace App { $raw = '\xff'; class One {} cl/**/ass invalid }\n\
            namespace { $name = App\\One::class; class GlobalType {} }";
        assert_eq!(super::names(source).unwrap(), ["App\\One", "GlobalType"]);
    }
}
