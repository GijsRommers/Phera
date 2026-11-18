# Random numbers in Phera

This crate is PHP's random extension: `mt_rand()`, `rand()` and their
seeding functions, `random_int()`, `random_bytes()`,
`uniqid()`, `shuffle()`, `str_shuffle()`, `array_rand()`, and the
`Random\Randomizer` class with the engines `Mt19937`, `PcgOneseq128XslRr64`,
`Xoshiro256StarStar` and `Secure`, the two engine interfaces and the three
exception classes.

```php
mt_srand(42);
mt_rand(1, 6);   // the number PHP gives

$randomizer = new Random\Randomizer(new Random\Engine\Xoshiro256StarStar(42));
$randomizer->getInt(1, 6);              // the number PHP gives
$randomizer->shuffleArray(['a', 'b', 'c']);
random_int(1, 6);                        // from the operating system
```

A seeded engine produces PHP's sequence exactly. `mt_rand()`, `rand()`, `shuffle()`,
`str_shuffle()` and `array_rand()` share one Mt19937 per request, seeded at
random until the script seeds it; requests never share one.
`random_int()`, `random_bytes()` and the `Secure` engine read the operating
system's generator through `getrandom`, one system call each time.

The engines, the float sampling and the order in which shuffles and key
picks draw are ported from php-src and keep its licences, in `LICENSE-PHP`
and `LICENSE-MT19937`.

## Differences from PHP

- **`Random\IntervalBoundary`** does not exist, because Phera has no enums
  yet. `getFloat()` works with its default boundary (closed at the low end,
  open at the high end); passing any `$boundary` is a `TypeError`.
- **Engines written in PHP** cannot be used: a Randomizer takes the four
  built-in engines only.
- **`Randomizer::$engine`** is an ordinary public property, not readonly.
- **Engine objects** cannot be cloned (the clone has no state and fails on
  first use), serialized or dumped with their state; cloning `Secure` is not
  refused as it is in PHP.
- **`mt_rand()` and `rand()`** accept no arguments or two. Their signature
  is declared as variadic to check that the way PHP does, so reflection
  shows a variadic parameter.

## How it is checked

`cargo test -p phera-ext-random` compares the extension with PHP 8.5, found
as described in CONTRIBUTING.md: a million draws per engine for ten range shapes, through a SHA-256 of all of them and the
first and last hundred one by one (30 million draws); every Randomizer
method with valid and rejected arguments, and engine jumps (5,127 calls);
the procedural shuffles and key picks after seeding;
the aliases and request isolation (10,000 draws); argument errors. The
secure functions are checked for plausibility: 100,000 draws of
`random_int(0, 15)` must fill all sixteen values evenly.
