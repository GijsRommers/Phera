--TEST--
A generator preserves its state around fiber switches
--FILE--
<?php
function elements() {
    $fiber = new Fiber(fn () => Fiber::suspend('child'));
    yield $fiber->start();
    $fiber->resume(7);
    yield $fiber->getReturn();
}
var_dump(iterator_to_array(elements()));
--EXPECT--
array(2) {
  [0]=>
  string(5) "child"
  [1]=>
  int(7)
}
