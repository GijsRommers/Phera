--TEST--
invoke anonymous function
--FILE--
<?php
class Invokable { function __invoke(int $n) { return $n + 1; } } $object = new Invokable;
$f = function ($n) { return $n * 2; }; var_dump(array_map($f, [1, 2]));
--EXPECT--
array(2) {
  [0]=>
  int(2)
  [1]=>
  int(4)
}
