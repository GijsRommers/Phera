--TEST--
invoke native callback
--FILE--
<?php
class Invokable { function __invoke(int $n) { return $n + 1; } } $object = new Invokable;
var_dump(array_map($object, [1, 2]));
--EXPECT--
array(2) {
  [0]=>
  int(2)
  [1]=>
  int(3)
}
