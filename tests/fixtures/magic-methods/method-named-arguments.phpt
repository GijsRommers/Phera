--TEST--
Magic call arguments retain named keys
--FILE--
<?php
class Caller { function __call($name, $args) { var_dump($name, $args); } }
(new Caller)->Missing(1, third: 3, second: 2);
--EXPECT--
string(7) "Missing"
array(3) {
  [0]=>
  int(1)
  ["third"]=>
  int(3)
  ["second"]=>
  int(2)
}
