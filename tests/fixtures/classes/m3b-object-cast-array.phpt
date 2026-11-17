--TEST--
Class members: object-cast-array
--FILE--
<?php var_dump((object)['a'=>1, 2=>3]);
--EXPECT--
object(stdClass)#1 (2) {
  ["a"]=>
  int(1)
  ["2"]=>
  int(3)
}
