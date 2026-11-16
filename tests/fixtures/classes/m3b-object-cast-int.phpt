--TEST--
Class members: object-cast-int
--FILE--
<?php var_dump((object)7);
--EXPECT--
object(stdClass)#1 (1) {
  ["scalar"]=>
  int(7)
}
