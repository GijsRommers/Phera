//! How deeply source may nest.
//!
//! ```php
//! <?php
//! $x = ((((((1))))));          // fine at any depth real code has
//! $x = [[[[ /* 9,000 more */ ]]]];
//! // PHP Parse error:  memory exhausted in ... on line 1
//! ```
//!
//! Long *chains* (`1 + 1 + ...`, `$a->b()->c()...`, a hundred thousand array
//! items or `elseif`s) are parsed in loops and never count here. Real
//! *nesting* does: each `(`, `[`, call or block inside another is one more
//! level of recursion, here and in every pass after the parser.
//!
//! Two limits apply, and the first one reached gives the error:
//!
//! - **PHP's own limit.** PHP's parser keeps every construct it is inside of
//!   on a stack of [`PHP_PARSER_STACK`] entries and stops with "memory
//!   exhausted" when that is full. A construct takes as many entries as PHP's
//!   grammar has symbols open at that point: one for `(`, two for a call's
//!   `f(`, six for `if (...) {`, twelve for a closure body. The parser counts
//!   the same way ([`Parser::with_php_slots`]), so it stops where PHP stops,
//!   to within a few levels.
//! - **Phera's own limit.** The parser, the compiler and the analyzer descend
//!   into nested code by recursion, and a stack overflow cannot be caught: it
//!   would take the whole process down. [`MAX_DEPTH`] caps that recursion. It
//!   is below PHP's limit for the constructs PHP nests most cheaply (it allows
//!   2,500 nested arrays where PHP allows 9,993), far above what any real code
//!   nests, and its cost fits the compiler's dedicated stack in a debug build
//!   (see `tests/long-sources/README.md` for the measurements).

use crate::error::ParseError;
use crate::parser::Parser;

/// Bison's `YYMAXDEPTH`, the size of PHP's parser stack.
const PHP_PARSER_STACK: u32 = 10_000;

/// How deep the recursive descent may go.
const MAX_DEPTH: u32 = 2_500;

/// The parser's position in both limits.
#[derive(Default)]
pub(crate) struct Nesting {
    depth: u32,
    php_stack: u32,
}

impl Parser<'_> {
    /// Enters a recursive parsing routine: one level deeper, one more entry
    /// on PHP's stack.
    pub(crate) fn enter(&mut self) -> Result<(), ParseError> {
        self.nesting.depth += 1;
        self.nesting.php_stack += 1;
        if let Err(error) = self.check_nesting() {
            self.leave();
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn leave(&mut self) {
        self.nesting.depth = self.nesting.depth.saturating_sub(1);
        self.nesting.php_stack = self.nesting.php_stack.saturating_sub(1);
    }

    /// Runs `parse` with `slots` more entries of PHP's parser stack in use:
    /// the symbols PHP's grammar holds open around this construct beyond the
    /// one level that [`Parser::enter`] counts.
    pub(crate) fn with_php_slots<T>(
        &mut self,
        slots: u32,
        parse: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        self.nesting.php_stack += slots;
        let result = self.check_nesting().and_then(|()| parse(self));
        self.nesting.php_stack = self.nesting.php_stack.saturating_sub(slots);
        result
    }

    fn check_nesting(&self) -> Result<(), ParseError> {
        if self.nesting.php_stack > PHP_PARSER_STACK {
            return Err(self.error("memory exhausted"));
        }
        if self.nesting.depth > MAX_DEPTH {
            return Err(self.error(format!(
                "nesting is too deep: Phera supports at most {MAX_DEPTH} levels"
            )));
        }
        Ok(())
    }
}
