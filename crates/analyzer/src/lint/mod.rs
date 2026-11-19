//! Source linting without PHP expression types.
//!
//! ```php
//! use App\Unused;
//! $ports = ['http' => 80, 'http' => 8080];
//! ```
//!
//! Rules register a stable name, a group, examples and the nodes they inspect.
//! A single explicit work stack visits the file. A rule can defer decisions
//! such as unused imports until the namespace's references have been seen.
//! A fix is a set of byte edits. The fixer applies compatible sets, parses the
//! result before saving it and checks that the edits introduce no findings.
