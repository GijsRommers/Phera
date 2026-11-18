--TEST--
Magic call accepts positional and named unpacked arguments
--FILE--
<?php
class Caller { static function __callStatic($name, $args) { var_dump($args); } }
Caller::run(...[1, 'second' => 2]);
--EXPECT--
array(2) {
  [0]=>
  int(1)
  ["second"]=>
  int(2)
}
