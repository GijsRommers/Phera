# Array index slots keep 32 bits of the key's hash; string writes probe once

- Date: 2026-11-27
- Area: Speed and method
- Decided by: Design

## Decision

The hash index of a large `PhpArray` map stores, per slot, the entry's
position and 32 bits of its key's hash (`Slot { position: u32, hash: u32 }`),
as PHP's buckets keep `h`. The table files a slot under those 32 bits put in
both halves of its 64-bit hash. A probe compares the 32 bits before it reads
the entry, and growing the table never hashes a key again.

`$array[$string] = $value` in the interpreter's fast path inserts with one
hash and one probe (`PhpArray::insert_if_not_ref`) instead of a failed lookup
followed by an insert, which hashed the key and checked it for a numeric
string twice.

## Why

In `Zend/bench.php`'s `hash1`, 9.3% of the time was hashbrown re-hashing
every key (foldhash over the key's bytes) each time the index grew, because
the index held bare positions. Per entry the index grows from 4 to 8 bytes;
an entry is 32 bytes, so a large map grows by about 10%.

## Turned down

- **Caching a hash inside `PhpString`.** It would make every string bigger and
  every mutation clear it, for a cost that only growth paid.
- **A full 64-bit hash per slot.** 32 bits leave false matches at one in four
  billion after the control byte; the extra 4 bytes buy nothing.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| `Index::positions` | `HashTable<u32>` (position) | `HashTable<Slot>` (position, 32 hash bits) |
| Growth of the index | hashes every key again | reads the stored bits |
| `$a[$str] = $v`, new key | `get_mut_str` then `insert`: 2 hashes, 2 numeric checks, 2 probes | `insert_if_not_ref`: 1 of each |
