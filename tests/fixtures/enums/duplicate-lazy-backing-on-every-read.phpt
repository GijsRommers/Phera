--TEST--
enums: equal backing values found at run time are reported on every use of the enum
--FILE--
<?php
const SAME = "same";
enum Twice: string { case P = SAME; case Q = SAME; const K = 5; }
function attempt(string $label, callable $read): void {
    try { echo $label, ": "; var_dump($read()); } catch (Throwable $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
}
attempt("first case", fn() => Twice::P);
attempt("first case again", fn() => Twice::P);
attempt("second case", fn() => Twice::Q->value);
attempt("constant", fn() => Twice::K);
attempt("from", fn() => Twice::from("same"));
attempt("cases", fn() => count(Twice::cases()));
function again(): void { for ($i = 0; $i < 3; $i++) { attempt("in a loop", fn() => Twice::P); } }
again();
--EXPECT--
first case: Error: Duplicate value in enum Twice for cases P and Q
first case again: Error: Duplicate value in enum Twice for cases P and Q
second case: Error: Duplicate value in enum Twice for cases P and Q
constant: Error: Duplicate value in enum Twice for cases P and Q
from: Error: Duplicate value in enum Twice for cases P and Q
cases: int(2)
in a loop: Error: Duplicate value in enum Twice for cases P and Q
in a loop: Error: Duplicate value in enum Twice for cases P and Q
in a loop: Error: Duplicate value in enum Twice for cases P and Q
