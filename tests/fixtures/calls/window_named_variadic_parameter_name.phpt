--TEST--
Call window: named variadic parameter name
--FILE--
<?php
function f(...$rest){var_dump($rest);}f(rest:1,x:2);
--EXPECT--
array(2) {
  ["rest"]=>
  int(1)
  ["x"]=>
  int(2)
}
