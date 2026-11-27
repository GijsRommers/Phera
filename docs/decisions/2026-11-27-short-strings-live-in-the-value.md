# Short strings live in the value

- Date: 2026-11-27
- Area: Speed and method
- Decided by: Design

## Decision

`PhpString` is either inline or shared. A string of at most 14 bytes, the
empty string included, is kept in the value itself (`Inline { len, bytes:
[u8; 14] }`); a longer one is `Rc<Vec<u8>>` as before, shared by its copies
until one of them changes it. The length is an enum of its 15 values, so its
byte has 241 values a length never takes, and the compiler keeps the variant
of `PhpString`, `Value` and `ArrayKey` in them: all three stay 16 bytes, and
`Option<Value>` too (asserted at compile time).

The API of `PhpString` is unchanged (`as_bytes`, `push_bytes`,
`make_mut_bytes`, `into_bytes`, `Deref<Target = [u8]>`, ...), so no
`Value::String` site changed. Two heap strings compare by pointer first.

`dechex`, `decbin` and `decoct` of an int run in the interpreter loop (the
`PureNative` built-ins), with the digits written into a 64-byte array by a
shared `power_of_two_digits`; their results are short strings.

## Why

Every string a program makes cost two allocations (the `Rc` and the `Vec`'s
buffer) and two frees, however short. Short strings are most strings that are
made at run time: array keys (`"foo_$i"`, `dechex($i)`), concatenations of
names, JSON and database fields. `hash1` of `Zend/bench.php` made and freed
100,000 of them, and left the loop 100,000 times for `dechex`.

## Turned down

- **A `Value::InlineStr` variant beside `Value::String` (and
  `ArrayKey::Inline`).** About 330 matches on `Value::InternedStr` and 600 on
  `Value::String` would need a third arm, and a match with a `_` arm would
  quietly treat a short string as no string at all. Inside `PhpString` the
  representation is invisible to them.
- **`Rc<[u8]>` for every string.** One allocation instead of two, but `.=`
  would copy the string each time.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| `PhpString` | `Rc<Vec<u8>>`, 8 bytes, two allocations | inline up to 14 bytes, else `Rc<Vec<u8>>`; 16 bytes |
| Empty string | an `Rc` allocation | inline, nothing allocated |
| `dechex` / `decbin` / `decoct` | leave the loop for the built-in | answered in the loop for an int |
