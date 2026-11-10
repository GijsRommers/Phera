//! Parsing PHP source into the AST defined by `phera-ast`.
//!
//! The parser is a hand-written recursive-descent parser with precedence
//! climbing (Pratt) for expressions. The grammar and the operator precedence
//! table mirror PHP's own `zend_language_parser.y` (PHP 8.5); left-associative
//! chains and postfix operators are parsed in loops, never by unbounded
//! recursion.

mod declaration;
mod error;
mod expression;
mod names;
mod parser;
mod statement;
mod strings;

pub use error::ParseError;

/// The stack the parser runs on. Only the part that is actually used gets
/// backed by memory, so the size costs address space, not RAM.
pub const PARSER_STACK_BYTES: usize = 256 * 1024 * 1024;

/// Parses a whole PHP file (inline HTML and `<?php` tags included) into a
/// [`phera_ast::Program`]. The first error stops parsing.
///
/// The work happens on a short-lived thread with its own large stack. The
/// caller may be on a small stack (a test thread, a server worker), and a stack
/// overflow cannot be caught: it would take the whole process down. On its own
/// stack the parser's nesting limit is guaranteed to trigger first.
pub fn parse(source: &str) -> Result<phera_ast::Program, ParseError> {
    let whole_file = phera_source::Span::new(0, source.len());

    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("phera-parser".into())
            .stack_size(PARSER_STACK_BYTES)
            .spawn_scoped(scope, || parse_on_current_stack(source))
            .map_err(|error| {
                ParseError::new(
                    format!("cannot start the parser thread: {error}"),
                    whole_file,
                )
            })?
            .join()
            .unwrap_or_else(|_| Err(ParseError::new("internal parser error", whole_file)))
    })
}

/// Like [`parse`], but on the caller's stack. Only for callers that already
/// run on a stack of at least [`PARSER_STACK_BYTES`], such as a compiler
/// thread that parses and compiles in one go.
pub fn parse_on_current_stack(source: &str) -> Result<phera_ast::Program, ParseError> {
    parser::Parser::new(source).parse_program()
}
