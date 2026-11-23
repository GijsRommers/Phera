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
| Fixed keys in enum and unserialize diagnostics | Interner lookup at each use | One process lookup and a retained per-thread key, including full-table fallback |

Canonical strings retain byte equality by pointer, precomputed truthiness, and
the existing interner bounds. Mutation first makes writable storage so it
cannot change a canonical string or its aliases. The allocation for a shared
vector and the four 16-byte size assertions keep frame and array slot sizes.

Keeping two value variants would keep teaching each extension about storage.
Adding a separate borrow wrapper would make each caller unpack the same
representation again. Both choices are unnecessary when `PhpString` owns the
distinction.

Trusted entry files, application preloads, and shipped code collect canonical
pointers when compiled. Native registrations and native class linking can
intern their fixed declarations. VM materialization reads prepared pointers.
Request-selected includes, stream code, eval, and dynamic property spelling
use shared storage. The bounded table may decline a trusted literal. A short
literal of request-selected code retains its debug spelling with a flag on
the shared handle.

An integer key still goes through process-seeded `RandomState::hash_one(i64)`.
For the supported x86-64 and AArch64 targets, foldhash multiplies two seeded
64-bit words into a 128-bit product and XORs its halves. The distribution test
covers 65,536 keys with only their top 16 bits changing. Flood resistance
rests on a process-wide seed. PHP uses no secret array-key seed.
