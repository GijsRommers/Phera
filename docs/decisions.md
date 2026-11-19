# Decisions

Every decision that shapes Phera is written down here: what was decided, why,
and what was turned down. The newest entries are at the top of each part. A
pull request that takes a decision adds its entry in the same pull request.

"Owner" means Gijs Rommers decided it. "Design" means it was decided during
design or review work and the owner can overrule it.

## Scope and compatibility

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | WordPress is a target next to Symfony and Laravel. `mysqli` and the `pg_*` functions are built as thin layers over the database drivers PDO already uses. | WordPress is the most used PHP application and talks to MySQL only through `mysqli`. One client per database keeps pooling, TLS and type mapping in one place. | A second, separate MySQL client for `mysqli`. | Owner |
| 2026-11-19 | Phera follows the newest PHP 8.5 patch release. A comparison whose PHP answer changed in a patch release is marked with the version it holds from and is skipped, visibly, on an older PHP. It is never removed. | Patch releases are bug fixes; CI moves to them on its own; nobody runs an old patch on purpose. | Pinning CI to one patch version. | Owner |
| 2026-11-19 | PHP 8.5 only. The two-version design for PHP 8.6 (architecture.md 6.3) is parked. | Decided by the owner after seeing the design. | Carrying 8.5 and 8.6 side by side now. | Owner |
| 2026-11-19 | `Fiber` stays as a complete class. It is a compatibility API (Monolog and others ask `Fiber::getCurrent()`), not part of Phera's concurrency, which is tasks on Tokio. | More real code runs unchanged. | Reducing it to `Fiber::getCurrent()` returning null. | Design |
| 2026-11-17 | A deprecated named item that nothing in use needs is not built and throws `Phera\NotSupportedException` or a compile error. Deprecated behaviour of a supported operation stays as in PHP, message included. | A clear error beats a silently different answer; the list is `crates/source/data/deprecations.tsv`. | Reproducing every legacy corner. | Owner |

## Safety and dependencies

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | TLS uses rustls with ring for databases, Redis and HTTP clients. PDO and Redis share TLS 1.2 and 1.3 policy in `phera-io-pool`. | Ring is audited, widely deployed and already in the dependency tree. Its bundled C and assembly sit behind safe Rust APIs. The removed pure-Rust provider was a pre-release that labeled itself unsuitable for production. | Keeping the pre-release provider or linking OpenSSL for TLS. | Owner |
| 2026-11-19 | The [Advisories job](../.github/workflows/ci.yml) scans the whole lockfile on pull requests and on `main`. The [ignore file](../.github/advisory-ignores.json) requires a reason for every exception. | New advisories fail the check. An existing exception stays visible until the dependency can be replaced. | Warning-only scans or a blanket exemption for maintenance advisories. | Owner |
| 2026-11-19 | RustSec flags `chrono` as unmaintained (RUSTSEC-2026-0543). Replace it with a maintained date API, then remove the advisory exception once `chrono` leaves the lockfile. | Preserve PostgreSQL date and timestamp decoding, HTTP cookie expiry formatting, and session cookie and cache-header dates during the migration. | Combining the date-compatibility migration with the TLS provider change. | Owner |
| 2026-11-19 | Cryptography (`openssl_*`, `sodium_*`, password hashing) is built from audited Rust crates. Phera implements no cipher, hash, padding or secret comparison itself. What no audited crate offers is left out and listed. | Home-made cryptography is where runtimes get broken. | Linking OpenSSL or libsodium. | Design |
| 2026-11-19 | DOM and XML are Phera's own safe tree with pure Rust parsers (`quick-xml`, `html5ever`), not libxml2. No external entity is ever loaded while parsing a string. `DOMDocument::loadHTML()` therefore differs from PHP for malformed HTML 4. | A C parser fed with documents from the network gives up the memory safety the runtime exists for. | Binding libxml2 for byte-exact output. | Design |
| 2026-11-19 | curl is built on `hyper` and rustls, not on libcurl. Only `http` and `https` exist; other protocols are refused with libcurl's error. | Same reason as DOM. Refusing `file://`, `gopher://` and the like also closes a class of request-forgery problems. | Binding libcurl. | Design |
| 2026-11-19 | `LOAD DATA LOCAL INFILE` is refused in PDO and `mysqli`. | It lets a database server read files of the client. | Supporting it behind an option. | Design |
| 2026-11-17 | No `unsafe` in Phera's own crates; a workspace lint forbids it and CI checks it. Where a design seems to need it, the answer is a safe design that may be slower. A specific use may be proposed to the owner for a gain of about ten times; it is never written without his decision. | The owner's rule. | | Owner |

## Server and requests

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | Worker mode is supported too, through FrankenPHP's interface (`frankenphp_handle_request()`), so that Laravel Octane, Symfony's runtime packages and FrankenPHP applications run unchanged. Phera keeps many running copies of the worker script per thread, each answering one request at a time, so that waiting stays cheap. | The owner wants applications written for long-running use to run as they are. An existing interface means no new package for anyone to write. | A Phera-only worker interface; one copy of the worker script per thread, which would give up many requests per thread. This reverses the first draft of the design, which left worker mode out. | Owner |
| 2026-11-19 | Snapshot and fork is built in four steps: linked code shared per worker; a snapshot at `Phera\snapshot()` with a copy per request; adapters for Symfony and Laravel; copy on first write only if measurements ask for it (architecture.md 5.4). | One Laravel request costs 10 to 17 times PHP's time, and most of that is work repeated per request that opcache does once. Step 1 helps every application; the snapshot removes the framework's boot. | Offering only worker mode: state leaks between requests unless the application is written for it, so it cannot be the default for every application. | Owner asked for it; design |
| 2026-11-19 | When a web request ends, terminate an abandoned child and its process group while Phera still tracks the group leader. | A child can retain descriptors and consume resources after the response or deadline of a request. | PHP-FPM leaves ordinary children running. Waiting for them would tie request cleanup to the life of the child. | Design |
| 2026-11-19 | Program execution is enabled by default. `phera serve --no-process` makes every program launch entry point throw `Error`, including `shell_exec()` and backticks. | Frameworks expect process functions to be available. Deployments that never launch children can disable them together. | Disabling them by default or requiring a list of individual functions. | Design |
| 2026-11-19 | CLI `pcntl_fork()` throws `Phera\NotSupportedException`. | Forking leaves one thread in the child, which inherits runtime and allocator locks that other threads may have held. | Calling Unix `fork()` and continuing to run the interpreter in the child. | Design |
| 2026-11-19 | The execution time limit counts the CPU time a request spends running PHP code and is checked where the interpreter already hands over to the scheduler. Process and process-stream waits also count toward the budget. Other native calls check the limit when they return control to the interpreter. | A check per instruction would slow every script; waiting for a database must not count, as in PHP on Unix. | A wall-clock limit; a clock read in the interpreter loop. | Design |
| 2026-11-19 | A multipart boundary longer than 70 bytes is refused, and parsing stops when the request is abandoned. | RFC 2046's limit. A very long boundary made parsing take minutes after the client had gone (security round 3). | | Design |

## Tools

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | The repository holds no directories of loose helper scripts. A helper worth keeping is a Rust test or a command of the tooling crate; a one-off stays outside the repository. Decisions go into this file, and the README is updated with every change a user can see. | The owner reads the repository to follow the project. | | Owner |
| 2026-11-19 | `phera lint` reports by default only what gave no false finding on the Symfony and Laravel code bases; unused variables, unused private properties, switch fall-through and self-comparison are among the defaults. Style, naming and complexity rules are opt-in. | A linter that cries wolf gets switched off. | All rules on by default. | Design |
| 2026-11-19 | `phera check` uses PHPStan's levels and identifiers, and reads a PHPStan baseline. | Projects can switch without relearning or rewriting their configuration. | Own level scheme. | Design |
| 2026-11-18 | Phera has no template engine of its own for now; Twig and Blade run as PHP. A native renderer is designed (architecture.md 11.6) and not built. | The frameworks' own engines must work anyway. | | Design |

## Speed and method

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | Speed is compared with PHP started as `php -n` (no Xdebug), with a second column for opcache and the JIT, and reported in cycles or CPU time next to instructions. | An earlier comparison used a PHP with Xdebug loaded and made Phera look faster than it is. Phera also retires fewer instructions per cycle than PHP, so instruction counts alone understate the gap. | Instruction counts as the only measure. | Design |
| 2026-11-19 | Built-in calls use the special forms PHP's own compiler has (`strlen`, `count`, type checks, `in_array` with a literal, `sprintf`, constant folding of pure functions). A shortcut tests properties of the call and never the spelling of a script. | They are observable in PHP only through speed, and each has a fixture proving equal behaviour, traces included. | Shortcuts tuned to benchmark scripts. | Design |
| 2026-11-18 | A change must not make any benchmark script more than 1 percent slower in instructions. The destructors work has a stated exception: no script more than 2 percent, at most three above 1 percent, the sum not slower, every overage named. | The interpreter loop is sensitive to layout; small costs add up unseen. Correct object lifetime has a real price. | No rule; a rule without exception. | Design |

## Process

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | CI runs the test suite as three jobs side by side, and a draft pull request gets the short checks, including Advisories. | One job took 20 to 24 minutes; work in progress filled the runners. | | Design |
| 2026-11-19 | Finished pull requests are merged into one batch branch, reviewed together and merged with one CI run. A batch merges only when a run of all php-src tests shows no test going from pass to fail. | Every merge makes the other branches out of date; the full php-src run finds regressions the repository's own selection does not. | Merging one by one. | Design |
| 2026-11-17 | Every pull request is reviewed against real PHP before it merges, whoever wrote it: its own scripts, the php-src tests, the benchmarks. The review is recorded on the pull request. | CI is only the first filter. | | Owner |

## Sodium

## 2026-11-19 — Register Sodium as a native extension

Register the PHP module and exception through the extension API. Use the existing
`base64` crate and the `hex` crate for the encoding functions. Match Sodium's
variant identifiers and ignore-string rules at the PHP boundary. The standard
extension's base64 decoder has different padding and invalid-input semantics.
The PHP oracle loads Sodium in both hosted test jobs so those semantics are
covered on Linux.

## 2026-11-19 — Use RustCrypto for Sodium's cryptographic operations

Use `crypto_secretbox` for both NaCl wire formats. Boxes derive the secretbox
key from X25519 with RustCrypto's HSalsa20. The `crypto_box` 0.9 implementation
reduces the clamped scalar modulo the base point order before multiplying
the peer key, which changed the shared key for non-canonical curve inputs
in the PHP oracle. Sealed boxes
derive their nonce with BLAKE2b over both public keys and use the recipient
public key from the PHP keypair during opening. Deriving a replacement public
key during opening would change the behavior of constructed keypairs. Reuse
OpenSSL's AES-GCM, ChaCha20-Poly1305, dalek, SHA-2 and randomness dependencies.
Use the stable secretbox crate. Its prerelease pins an incompatible
prerelease version of Salsa20.

Use the IETF AEAD interfaces for ChaCha20 and XChaCha20. The original
eight-byte-nonce AEAD uses the legacy ChaCha20 and Poly1305 implementations
with Sodium's length-field order. An IETF nonce with four prepended zeroes
produces a different authentication tag. The XChaCha20 stream's explicit
counter uses the separate 64-bit cipher interface, including Sodium's wrap
at the maximum counter. AES-256-GCM availability follows
Sodium's AES and polynomial-multiply feature check. Advertising the software
backend on unsupported CPUs would change PHP's availability result.

Use BLAKE2b parameter blocks for Sodium's KDF and BLAKE2b over the X25519 shared
secret and both public keys for KX. HKDF and labeled HMAC give different keys.
Authentication truncates HMAC-SHA-512 to 32 bytes. SipHash-2-4 returns the
eight little-endian bytes produced by Sodium.

## 2026-11-19 — Reuse Argon2 and implement the scrypt password format

Reuse the same Argon2 implementation as PHP's existing `password_hash()`
support. Sodium has its own memory units, default algorithm, and parameter
validation. Run Argon2 and scrypt off the request thread. Translate Sodium's
scrypt work limits into its `N`, `r` and `p` parameters and encode the
`$7$` string format. The scrypt crate's PHC string format does not
interoperate with PHP's scrypt password strings.

## 2026-11-19 — Preserve reference mutation and copy-on-write

Use constant-time comparisons from `subtle` and `crypto-bigint`. Increment,
add, and Ristretto scalar addition use the latter's fixed-precision unsigned
arithmetic. Reducing Ristretto inputs first changes Sodium's wrapping behavior
for non-canonical scalars. Erase a PHP
string in place when its backing buffer has a single owner, and set the
reference to null. Shared values retain their bytes. A raw-reference parameter exposes PHP's
declared string type while keeping Sodium's type checks on the original value.
Generic string coercion would turn values such as integers into strings.
Wiping a temporary copy
would leave the original allocation intact.

## 2026-11-19 — Publish the implemented PHP surface

Use the Sodium 1.0.22 constant values observed with the local PHP 8.5 oracle.
Its library version constants describe the compatibility target. Sodium is a
Rust module, so it has no linked libsodium version to query. The PHP patch
does not govern the library version reported by a PHP build.

Streaming BLAKE2b requires a compatible string representation for the internal
state. The selected BLAKE2b crate encapsulates its state. Buffering the entire
message would give streaming callers unbounded memory use, and an external
handle would change PHP copy and serialization behavior. Defer those calls
until the state can be represented safely and compatibly.

Defer AEGIS and secretstream until a compatible audited pure Rust backend is
available. Expose secretstream's independent OS-random key generator.
The RustCrypto secretstream API keeps explicit rekey and state
serialization private. Wrapping it as a PHP string would also change cloning
and serialization. The available `block-padding` ISO 7816-4 decoder exits early
on padding bytes, so it cannot provide Sodium's constant-time unpadding.

## 2026-11-19 — Omit secret arguments from Sodium exception traces

SodiumException and the Sodium parsing errors that PHP redacts now record an
empty argument list in each visible trace frame. Register this behavior as
native metadata. The VM can apply it before capturing frame arguments, and
other exceptions retain the existing trace behavior. Redacting individual
sensitive parameters would still expose a secret passed through a PHP caller.

## 2026-11-19 — List Sodium functions in PHP's extension order

Record Sodium's function order in the extension introspection catalog. The
runtime filters this catalog against the actual registered functions. This
preserves PHP's ordering without exposing functions whose backends are pending.

## 2026-11-19 — Keep dalek's Ed25519 key validation and RustCrypto's integer width

Signing uses dalek's expanded secret key operation so that a decodable embedded
public key takes part in the signature as it does in PHP. Dalek still rejects
embedded public bytes that cannot be decoded as a point. Reimplementing the
signature transcript to accept malformed keys would move a primitive into Phera.

Sodium's little-endian integer helpers use the width accepted by RustCrypto's
fixed-precision constructor. Their input size is therefore below 512 MiB.
Replacing the constant-time arithmetic with variable-time big integers would
change the security behavior of those helpers.

## 2026-11-19 — Serialize bounded, copyable Sodium hash state

Store BLAKE2b chaining words, the compressed byte count, and at most one
pending block in a 384-byte string, matching the length of PHP's opaque
state. Include a version marker and a checksummed payload. Each call validates
the representation before constructing a native state. PHP exposes no state
size constant. Preserve PHP's independent digest lengths at initialization
and finalization by retaining all chaining words through the final compression.

The earlier deferral is superseded. `blake2b_simd` keeps state and compression
internal. Use the documented EIP-152 BLAKE2F interface in
`eth-valkyoth-evm-core` with default features disabled. Its fixed-size input
contains the chaining words, pending block, byte count and final flag. Ask it
to run exactly 12 rounds. The implementation was covered by the project's
[external BLAKE2F review](https://github.com/valkyoth/eth/blob/main/security/pentest/v0.51.0.md).

REVM also exposes BLAKE2b compression. Its elliptic-curve dependency tree
introduces the vulnerable `crypto-bigint` 0.5.5 and the unmaintained `paste`
macro. The dependency-free BLAKE2F interface removes both. The standalone
`eip-152` package has a GPL-3.0 license. An in-memory handle would break value
copies, and retaining the full message would give callers unbounded memory use.

## 2026-11-19 — Compose secretstream using the libsodium transcript

Use RustCrypto HChaCha20, IETF ChaCha20, and Poly1305 for the documented
secretstream transcript and ratchet. Keep the derived key, counter, nonce and
checksum in a 52-byte opaque string. The checksum diagnoses accidental
corruption and allows state strings to resume across processes. The runtime
checks a frame's MAC before decrypting or advancing the state. Keep the
ciphertext-dependent zero padding of libsodium's historical transcript for
wire compatibility. The standard IETF AEAD computes a different MAC on
unaligned messages, and RustCrypto's secretstream wrapper keeps its state
and explicit rekey private. This supersedes the previous secretstream
deferral.

## 2026-11-19 — Use full-width byte operations and scan ISO/IEC 7816-4 padding

Use a carry over the complete little-endian PHP string for addition and
increment. Use `subtle` to select the last differing byte for comparison and
scan the complete final block for unpadding. This removes the integer
constructor's width limit and the early exit in the block-padding decoder.
Continue to use `block-padding` for encoding ISO/IEC 7816-4.

## 2026-11-19 — Retain the Ed25519 and AEGIS boundaries

Dalek requires a decodable public point for its expanded-key signing API.
Deriving the public half from the seed changes the signature transcript
when the stored half differs. Keep dalek's rejection for undecodable points
and document it on both signing operations.

The `aegis` crate offers an optional pure Rust backend. A published audit
covering that backend could not be verified. Keep the PHP compatibility
constants and leave the six AEGIS functions absent. The crate's default
configuration uses C.

## 2026-11-19 — Preserve backend dispatch for secretbox

Secretbox reaches `salsa20` 0.10.2 and `poly1305` 0.8.0 through
`crypto_secretbox`. Salsa20 uses a portable backend. Poly1305 uses its
portable backend on Apple silicon and detects AVX2 at runtime on x86.
Keep portable builds and runtime feature selection. Forcing CPU flags
would narrow which machines can run the binary.
