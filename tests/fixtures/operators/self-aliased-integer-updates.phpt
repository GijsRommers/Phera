--TEST--
Integer updates preserve self-aliasing, increment order and overflow
--FILE--
<?php
function updates() {
    $n = 6;
    $n += $n;
    var_dump($n);
    var_dump($n++, ++$n, $n--, --$n);

    $n = PHP_INT_MAX;
    $n += 1;
    var_dump(is_float($n), $n > 0);
    $n = PHP_INT_MIN;
    $n--;
    var_dump(is_float($n), $n < 0);
}
updates();
--EXPECT--
int(12)
int(12)
int(14)
int(14)
int(12)
bool(true)
bool(true)
bool(true)
bool(true)
