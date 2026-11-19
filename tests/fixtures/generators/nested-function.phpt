--TEST--
Only the function that contains a yield becomes a generator
--FILE--
<?php
function wrapper(): int {
    $f = fn () => yield 9;
    return $f()->current();
}
function &early_return() {
    return $missing;
    yield 1;
}
var_dump(wrapper(), early_return()->getReturn());
--EXPECTF--

Warning: Undefined variable $missing in %s on line 7
int(9)
NULL
