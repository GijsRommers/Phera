# Unify interned and shared string values

Keep one string variant in PHP values and array keys. Dispatch on storage in
`PhpString` so extensions share the same byte, comparison, and mutation code for
literals and built strings.

| Structure | Was | Is |
| --- | --- | --- |
| `Value` and `ArrayKey` | One variant per string representation | One string variant holding a `PhpString` |
| String storage | A canonical pointer or an `Rc<Vec<u8>>` chosen by the value | The same two forms inside a private string representation |
| String lookup in arrays | A function per representation | Borrow a `PhpString` and use its precomputed hash and numeric-key classification when it is interned |
| String borrow | A separate `StrRef` wrapper | Borrow a `PhpString` |

Canonical strings retain byte equality by pointer, precomputed truthiness, and
the existing interner bounds. Mutation first makes writable storage so it
cannot change a canonical string or its aliases. The allocation for a shared
vector and the four 16-byte size assertions keep frame and array slot sizes.

Keeping two value variants would keep teaching each extension about storage.
Adding a separate borrow wrapper would make each caller unpack the same
representation again. Both choices are unnecessary when `PhpString` owns the
distinction.
