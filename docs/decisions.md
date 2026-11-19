# Design decisions

## 2026-11-19

### Decimal arithmetic uses integer coefficients

BCMath stores each value as a signed `num-bigint` coefficient and an explicit
decimal scale. It aligns coefficients for addition and comparison and uses
integer division to truncate toward zero. This preserves trailing zeroes in
`BcMath\Number` while supporting computations beyond machine integer precision.
Floating point would lose decimal digits. A general decimal crate would still
require separate rules for PHP's truncation, rounding modes, and result scales.

### Native binary operators run after scalar dispatch

Native classes can handle an operator with both original operands through
`ClassHooks::binary`, or decline and let the VM apply the usual PHP rules.
The VM checks for a native hook after the scalar fast path and uses its existing
slow instruction dispatch. This keeps operand order for noncommutative
operations and avoids hard-coding BCMath or GMP arithmetic in the VM.

### GMP uses Rust integer algorithms and a request-local generator

GMP shares the `num-bigint` representation and uses `num-integer` for
rounding divisions, modular arithmetic, and the Euclidean algorithm. It uses
Miller–Rabin over the same representation for primality. Integer string
conversion implements PHP's base and prefix rules before parsing the digits.
A C GMP binding would reproduce its internal implementation but add a native
dependency. The generator implements GMP's Mersenne Twister seeding and
output algorithm because a different generator would break seed replay.

### Scalar casts use the existing value conversion paths

Native classes supply optional scalar casts through their class handle. This
keeps explicit casts and truth tests consistent and avoids adding GMP-specific
branches to scalar bytecode. The VM uses the same native dispatch for GMP
unary operators, leaving ordinary unary operations with the value layer.

The compiler's literal operands and indexed compound assignments share the
same native dispatch as ordinary operators. This preserves overloading where
an instruction has a scalar fast path but sees an object at runtime.

### zlib uses the HTTP server's pure Rust DEFLATE backend

The extension uses the workspace's `flate2` with its `miniz_oxide` backend.
Explicit gzip and zlib framing preserves checksums and gives the decoder
control over output bounds and consumed bytes. The encoder emits stored
blocks directly for one-shot level zero. Using the platform's C zlib would
add a native dependency and tie the implementation to its version. The
version constants expose the targeted zlib 1.2.12 interface.

### Preset dictionaries are supplied as DEFLATE history

A non-final stored block seeds the decoder with dictionary bytes before it
processes input. Checksums and reported input length start with the caller's
data. The encoder can advertise a preset dictionary without requiring the
backend to reference it. Replacing the backend solely for dictionary and
compression-strategy tuning would diverge from the server's existing dependency.

### gzip files reuse the PHP stream resource

The gzip backing handles transparent reads and writes and shares stream
position, modes and aliases. Gzip I/O and close run through Tokio's blocking
pool. A separate resource type would break mixing `gzread` with `fclose` or
ordinary stream operations. Gzip output buffering is explicit through
`ob_gzhandler`; the INI setting stores configuration without installing a
buffer at request startup. The streams crate has no filter registration API.
