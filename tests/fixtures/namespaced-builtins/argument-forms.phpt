--TEST--
Named and spread arguments, wrong argument counts and first-class callables keep the general call.
--FILE--
<?php
// Named and spread arguments, wrong argument counts and first-class
// callables keep the general call.
namespace App;
$values = [null];
for ($round = 0; $round < 2; $round++) {
    var_dump(is_null(value: null), is_null(...$values), count([1, [2, 3]], COUNT_RECURSIVE));
    $test = is_null(...);
    var_dump($test(null), $test(1));
    foreach ([fn () => is_null(), fn () => strlen('a', 'b'), fn () => is_null(nope: 1)] as $call) {
        try { var_dump($call()); } catch (\Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
    }
}
--EXPECT--
bool(true)
bool(true)
int(4)
bool(true)
bool(false)
ArgumentCountError: is_null() expects exactly 1 argument, 0 given
ArgumentCountError: strlen() expects exactly 1 argument, 2 given
Error: Unknown named parameter $nope
bool(true)
bool(true)
int(4)
bool(true)
bool(false)
ArgumentCountError: is_null() expects exactly 1 argument, 0 given
ArgumentCountError: strlen() expects exactly 1 argument, 2 given
Error: Unknown named parameter $nope
