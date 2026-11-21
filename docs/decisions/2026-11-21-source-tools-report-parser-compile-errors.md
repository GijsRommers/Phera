# Report parser compile errors in the source tools

- Date: 2026-11-21
- Area: Parsing and diagnostics
- Decided by: Design

`phera check` and `phera lint` use the parser's file compilation entry
point. They report errors that prevent that file from compiling,
including ambiguous ternaries, invalid loop jump levels and invalid
namespace layouts. They retain their existing analysis and reporting
formats. The parser still offers syntax-only parsing to callers that
need an AST without these compiler checks.

Namespace layout is checked in the compilation parse. It records
whether the first namespace uses braces and reports a conflicting
declaration at that declaration's line. PHP 8.5.12 rejects both orders
of mixed bracketed and unbracketed declarations.
