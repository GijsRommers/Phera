# One string type with interned literals

`Value::String` and `ArrayKey::String` hold the same `PhpString` type.
Its private representation distinguishes canonical immutable bytes from
reference-counted storage. A canonical entry stores the process-seeded hash
and the result of checking PHP's decimal integer key spelling. A shared
string caches its hash on demand. Mutation invalidates the hash and leaves
aliases and canonical bytes intact.

| Structure | Was | Is |
|---|---|---|
| PHP string handle | One `Rc<Vec<u8>>` | A canonical pointer or an `Rc` with a cached hash and bytes |
| PHP value and array key | 16 bytes each | 16 bytes each, with a 16-byte string handle |
| Array index | A seed per array | One secret seed per process |
| Integer key hash | Per-array foldhash | XOR the key with a seed-derived word, multiply by `0x9e3779b97f4a7c15` as a 128-bit number, then XOR the high and low halves |
| Array read | Materialize an owned key | Borrow an integer or string directly through one lookup |

A string hash of zero is recorded as one. All other string hashes retain all
their bits. The index's flood resistance rests on one randomized seed for
the process. An observed collision pattern can therefore be reused across
arrays in that process. PHP uses no secret seed for its array keys.

The bounded interner follows the 131,072-entry, 16 MiB payload, and 4,096-byte
single-string limits from #214. Host-selected entry files, preloaded files,
shipped PHP and registered declarations prepare canonical pointers while
compiling or linking. An evaluated string, stream include, or new file selected
during a request uses shared storage. Request loads only consult the code's
prepared metadata. A literal that falls back to shared storage keeps its
PHP debug output. Fixed enum lookup keys are retained after initialization.

Separate string variants in values and array keys would make every extension
dispatch on storage representation. The representation stays inside
`PhpString`. The borrowed array operations come from #212.
