--TEST--
Deep PHP recursion can suspend and resume inside a fiber
--FILE--
<?php
function wait_at_bottom($depth) {
    if ($depth === 0) return Fiber::suspend('bottom');
    return wait_at_bottom($depth - 1) + 1;
}
$fiber = new Fiber(fn () => wait_at_bottom(15000));
var_dump($fiber->start(), $fiber->resume(10), $fiber->getReturn());
--EXPECT--
string(6) "bottom"
NULL
int(15010)
