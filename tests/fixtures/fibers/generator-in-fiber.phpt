--TEST--
A generator suspended inside a fiber retains its activation
--FILE--
<?php
function elements() { yield 1; yield Fiber::suspend('from generator'); }
$fiber = new Fiber(fn () => iterator_to_array(elements()));
var_dump($fiber->start(), $fiber->resume(2), $fiber->getReturn());
--EXPECT--
string(14) "from generator"
NULL
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
