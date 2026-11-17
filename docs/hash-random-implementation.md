# Hash, password and random extension implementation

Branch: `ext/hash-random`. PR: https://github.com/GijsRommers/Phera/pull/11.

This is a **draft, incomplete delivery**. All 60 advertised hash algorithms produce verified
results, and the deterministic random engines match PHP across 40 million draws. The full
compatibility and performance gates are not met. The six short-secret xxHash oracle cases remain
failing tests; they are not ignored or marked as expected failures.

## Function and class status

DONE means the requested behavior is implemented and the described comparisons pass. It does not
mean every possible PHP program has been tested. Shared engine limitations below still apply.

| Function or class | Status | Scope or missing behavior |
| --- | --- | --- |
| `hash` | PARTIAL | All 60 algorithms, binary/hex and seeds/secrets work; six invalid-secret cases cannot be compared because PHP hangs. |
| `hash_file` | PARTIAL | Async local files; PHP stream wrappers are unavailable. |
| `hash_hmac` | DONE | All 44 cryptographic algorithms and variable key lengths. |
| `hash_hmac_file` | PARTIAL | Async local files; PHP stream wrappers are unavailable. |
| `hash_algos` | DONE | Exactly PHP's 60 algorithms, in PHP's order; no placeholders. |
| `hash_hmac_algos` | DONE | Exactly PHP's 44 HMAC algorithms. |
| `hash_equals` | DONE | Strict string validation and `subtle::ConstantTimeEq`. |
| `hash_pbkdf2` | DONE | Binary/hex, odd hex lengths, defaults and validation. |
| `hash_hkdf` | DONE | Binary result, defaults and validation. |
| `HashContext` | PARTIAL | Final native class, private constructor; VM clone/serialization hooks are absent. |
| `hash_init` | DONE | HMAC and algorithm options, including Murmur/xxHash seeds. |
| `hash_update` | DONE | Incremental state; uncommon algorithms buffer input until finalization. |
| `hash_update_file` | PARTIAL | Async local files; stream contexts/wrappers unavailable. |
| `hash_update_stream` | PARTIAL | Registered, validates nonresource arguments; actual stream resources unavailable and untested. |
| `hash_final` | DONE | Binary/hex and finalized-context rejection. |
| `hash_copy` | DONE | Independent native state copy. |
| `md5` | DONE | Binary/hex. |
| `md5_file` | PARTIAL | Async local files; PHP stream wrappers unavailable. |
| `sha1` | DONE | Binary/hex. |
| `sha1_file` | PARTIAL | Async local files; PHP stream wrappers unavailable. |
| `crc32` | DONE, pre-existing | Remains registered only in standard; PHP takes no binary flag for this function. |
| `password_hash` | DONE | Bcrypt `$2y$`, Argon2i/id v19, options and secure salts. |
| `password_verify` | DONE | PHP-to-Phera and Phera-to-PHP interoperability; wrong/malformed hashes. |
| `password_needs_rehash` | DONE | Algorithm and stored cost/options comparison. |
| `password_get_info` | DONE | Algorithm, algorithm name and stored options. |
| `password_algos` | DONE | `2y`, `argon2i`, `argon2id`. |
| `crypt` | PARTIAL | Requested `$2y$`, `$5$`, `$6$`, `$1$`, DES/extended DES work; PHP's extra `$2x$` compatibility variant is absent. |
| `random_int` | DONE | OS entropy, unbiased signed ranges and errors. |
| `random_bytes` | DONE | OS entropy and length validation. |
| `mt_rand` | DONE | Standard/legacy generators, PHP rejection mapping and argument rules. |
| `mt_srand` | DONE | Request state and legacy-mode deprecation. |
| `mt_getrandmax` | DONE | 2147483647. |
| `rand` | DONE | Shared MT state and PHP's reversed-range handling. |
| `srand` | DONE | Shared MT state and legacy-mode deprecation. |
| `getrandmax` | DONE | 2147483647. |
| `lcg_value` | DONE | Combined LCG, range checks and PHP 8.4 deprecation. |
| `uniqid` | DONE | Prefix/time/optional entropy formatting; async yield between identical microseconds. |
| `shuffle` | DONE | By-reference result and shared request MT state. |
| `str_shuffle` | DONE | Byte shuffle and shared request MT state. |
| `array_rand` | DONE | Single/multiple keys and holes left by unset. |
| `Random\Randomizer` | PARTIAL | Built-in engines work; custom userland engines, readonly engine property and serialization are incomplete. |
| `Random\Randomizer::nextInt` | DONE | Built-in engines. |
| `Random\Randomizer::nextFloat` | DONE | Built-in engines. |
| `Random\Randomizer::getInt` | DONE | Built-in engines and signed rejection mapping. |
| `Random\Randomizer::getFloat` | PARTIAL | Default ClosedOpen works; no enum support for other boundaries. |
| `Random\Randomizer::getBytes` | DONE | Built-in engines. |
| `Random\Randomizer::getBytesFromString` | DONE | Built-in engines and byte rejection sampling. |
| `Random\Randomizer::shuffleArray` | DONE | Built-in engines. |
| `Random\Randomizer::shuffleBytes` | DONE | Built-in engines. |
| `Random\Randomizer::pickArrayKeys` | DONE | Built-in engines. |
| `Random\Engine\Mt19937` | PARTIAL | Constructor/generate and both modes work; native clone/serialize/debug behavior absent. |
| `Random\Engine\PcgOneseq128XslRr64` | PARTIAL | Constructor/generate/jump work; native clone/serialize/debug behavior absent. |
| `Random\Engine\Xoshiro256StarStar` | PARTIAL | Constructor/generate/jump/jumpLong work; native clone/serialize/debug behavior absent. |
| `Random\Engine\Secure` | PARTIAL | OS generator works; PHP's uncloneable/unserializable class restrictions absent. |
| `Random\Engine` | DONE | Native interface registration. |
| `Random\CryptoSafeEngine` | DONE | Native interface registration extending Engine. |
| `Random\RandomError` | DONE | Extends Error. |
| `Random\BrokenRandomEngineError` | DONE | Extends RandomError. |
| `Random\RandomException` | DONE | Extends Exception. |
| `Random\IntervalBoundary` | NOT DONE | Compiler and native class API have no enums. |

Password constants are implemented: `PASSWORD_DEFAULT`, `PASSWORD_BCRYPT`, `PASSWORD_ARGON2I`,
`PASSWORD_ARGON2ID`, `PASSWORD_BCRYPT_DEFAULT_COST=12`,
`PASSWORD_ARGON2_DEFAULT_MEMORY_COST=65536`, `PASSWORD_ARGON2_DEFAULT_TIME_COST=4`,
`PASSWORD_ARGON2_DEFAULT_THREADS=1`, and `PASSWORD_ARGON2_PROVIDER='standard'`.
Both MT constants exist. The mode deprecation is emitted when seeding, but the native constant API
cannot emit PHP's separate deprecation when `MT_RAND_PHP` itself is read.

`shuffle`, `str_shuffle` and `array_rand` were absent in standard and are registered in random.
Standard already registered `crc32`; it was not duplicated or edited by this implementation.

## Verification

Oracle: real PHP 8.5.12, launched with `-n`, discovered by tests through
`phera_extension_api::testing::oracle::find_php()`. Tests compare returns and captured diagnostics;
the procedural shuffle tests include by-reference results. Password salts and secure randomness
are checked through interoperability/properties rather than impossible byte equality.

| Comparison suite | Calls/draws | Result |
| --- | ---: | --- |
| Every hash algorithm, lengths 0..300 and 511/512/513/1023/1024/1025/1 MiB, binary and hex | 36,960 | Zero mismatches |
| All HMAC algorithms, lengths 0..300 and ten key lengths spanning every block size | 264,880 | Zero mismatches |
| All algorithms, each split of 200 bytes, binary/hex and independent copy | 24,120 | Zero mismatches |
| Hash/KDF/options/password metadata parameter matrix | 1,584 | Zero mismatches |
| Async files and incremental files | 39 | Zero mismatches |
| `crypt` schemes, passwords and malformed salts | 56 | Zero mismatches |
| Password options and malformed verification | 44 | Zero mismatches |
| MT standard, MT legacy, PCG and Xoshiro; ten range shapes each | 40,000,000 | SHA-256 checksum and first/last 100 match |
| Procedural aliases and request isolation | 10,000 | Zero mismatches |
| Randomizer methods and engine jumps | 5,127 | Zero mismatches |
| Secure function argument errors | 7 | Zero mismatches |
| Procedural shuffles/key selection | 640 | Zero mismatches |
| LCG deprecation | 1 | Zero mismatches |
| Invalid short-secret xxHash with 1 KiB input | 6 | Oracle timeout failures retained |

Completed deterministic comparisons: **40,343,458 calls/draws, zero returned-value mismatches**.
There are also **six unsuccessful comparisons** in one failing test, so the complete test gate
is red. Counts exclude duplicate reruns and setup calls.

Additional checks: 12 password interoperability cases (60 explicit assertions), 494 static reference
vectors covering all algorithms, 100,000 secure integer samples, 4,096-byte secure output checks,
2,000 unique-ID samples and 1,000 LCG samples. Static vectors include RFC/NIST expectations from
php-src test fixtures and do not invoke PHP. The runtime integration fixture compares actual PHP
execution with Phera's VM, including namespaced object construction, async passwords, native hash
state and irrelevant password options.

The oracle hangs inside `XXH3_update` for both `xxh3` and `xxh128` when secret lengths are 0, 1 or
135 and the input is 1,024 bytes. The test kills only its own oracle child after two seconds and
fails visibly. Phera returns the short-secret Error instead of reproducing the hang. These cases
must be resolved with a working oracle or an explicitly agreed compatibility policy before the
PR can become ready; no such exception has been assumed.

Local format, workspace Clippy with warnings denied, unsafe-source scan and runtime integration
checks pass. The full hash test command fails on the six cases above. Full local `make ci` was not
run: its prerequisite step creates database accounts, which this task expressly forbids.
Hosted Linux CI is the integration gate; its current state is reported on the PR.

## Dependencies and execution

- RustCrypto `digest`, `md2`, `md4`, `md-5`, `sha1`, `sha2`, `sha3`, `ripemd`, `whirlpool`,
  `gost94`: pure Rust cryptographic implementations, including both GOST parameter sets.
- `xxhash-rust`: pure Rust xxHash32/64/3/128 with seeds and secrets.
- `bcrypt`, `argon2`: password hashing; `pwhash`: PHP-compatible crypt schemes and DES.
- `subtle`: constant-time equality; `hex`: output encoding; `getrandom`: operating-system entropy.
- `tokio`: asynchronous chunked file reads, `uniqid` yielding and blocking password workers.
- `phera-extension-api`: native registration, argument preparation and request state.

HAVAL, Snefru, Tiger variants, Murmur and checksum/FNV/joaat code name their php-src sources in
comments and retain the PHP BSD license. No own unsafe code, C bindings or macro definitions were
introduced. Tables are split into small modules rather than embedding giant source files.

`password_hash`, `password_verify` and `crypt` are async native functions. They copy owned bytes
and options into `tokio::task::spawn_blocking`; PHP's non-Send Rc values stay on the request worker.
Common cryptographic incremental hashes/HMAC stream their digest state; uncommon algorithms buffer
input. File I/O is asynchronous in both cases, but rare-algorithm finalization can still use memory
proportional to the file and run CPU work on the request thread.

## Engine/API gaps

- No stream resource API or wrapper resolver: file functions cover local files only.
- `ClassKind` lacks enums; the compiler explicitly rejects enums. This prevents IntervalBoundary.
- `PropertyEntry` lacks readonly/type metadata for `Randomizer::$engine`.
- VM object cloning copies properties and drops native state. It supplies no native clone hook.
  Serialization/debug hooks and per-class clone restrictions also need integration.
- Userland engines would need async callback dispatch plus byte buffering and failure accounting
  throughout Randomizer. The current implementation handles the four built-in native engines only.
  This is unfinished extension work, not a claim that the engine has no callback facility.
- Native constant metadata has no deprecation hook. `mt_rand`/`rand` use a variadic registration
  workaround to enforce PHP's disjoint zero-or-two argument rule before scalar coercion; reflection
  of that signature is not fully faithful.

## Performance

See the measured table below. The benchmark uses the same PHP script on PHP and Phera release,
measures inside the process with `hrtime`, and retains results to prevent dead-code elimination.
The 16-byte workload performs one million calls; the 1 MiB workload performs 100 calls.

Measured on Apple M4 / macOS arm64, PHP 8.5.12 and the Rust release build. Values are
medians of three alternating runs, in seconds. Raw runs: `benchmarks/extensions/hash-results.json`.
No other tests or builds started by this task were running during the measurements.

| Function | Input / calls | PHP seconds | Phera seconds | Phera / PHP |
| --- | --- | ---: | ---: | ---: |
| `md5` | 16 bytes / 1,000,000 | 0.116255 | 0.285758 | 2.46x |
| `sha1` | 16 bytes / 1,000,000 | 0.115775 | 0.273607 | 2.36x |
| `hash('sha256', ...)` | 16 bytes / 1,000,000 | 0.234415 | 0.359785 | 1.53x |
| `hash('xxh3', ...)` | 16 bytes / 1,000,000 | 0.052506 | 0.182356 | 3.47x |
| `crc32` | 16 bytes / 1,000,000 | 0.027181 | 0.204761 | 7.53x |
| `md5` | 1 MiB / 100 | 0.130509 | 0.133252 | 1.02x |
| `sha1` | 1 MiB / 100 | 0.101633 | 0.081392 | 0.80x |
| `hash('sha256', ...)` | 1 MiB / 100 | 0.285274 | 0.195137 | 0.68x |
| `hash('xxh3', ...)` | 1 MiB / 100 | 0.004564 | 0.002469 | 0.54x |
| `crc32` | 1 MiB / 100 | 0.211800 | 0.427448 | 2.02x |

**Performance gate not met.** MD5, SHA1, xxh3 and the pre-existing standard crc32 exceed
2x PHP for small inputs. The existing crc32 also measures 2.02x PHP for 1 MiB inputs.
SHA256 passes both sizes; the other new hashes pass the 1 MiB workload. These are whole-runtime
measurements, so native-call/loop overhead is included. No claim is made that the hash algorithm
alone causes all of the small-input gap. Further optimization requires profiling and, for crc32,
a separately scoped change in the standard crate.
