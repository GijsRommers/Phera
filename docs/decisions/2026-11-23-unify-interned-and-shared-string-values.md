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

Main used process-seeded `RandomState::hash_one(i64)`, which also folds
a widening product. A histogram of 65,536 keys differing in their top 16
bits fell below the distribution threshold for some random multipliers.
The integer index now XORs its key with a seed-derived word, multiplies by
`0x9e3779b97f4a7c15` into 128 bits, and XORs the halves. Flood resistance
rests on a process-wide seed. PHP uses no secret array-key seed.

| Structure | Was | Is |
| --- | --- | --- |
| Integer-key hash | `foldhash::hash_one(i64)` with a seeded multiplier | Seeded key and fixed multiplier, with high and low halves XORed |


| Structure | Was | Is |
| --- | --- | --- |
| Shared string array hash | Recomputed for every lookup | Stored on the shared allocation and invalidated before mutation |
| Shared vector allocation | 40 bytes including reference counts | 48 bytes including the cached hash |

Only a hash of zero is recorded as one. The full width is retained for all
other hashes. Caching and the inline-byte layout are measured separately.
