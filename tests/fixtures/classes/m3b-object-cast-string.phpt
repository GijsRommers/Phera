--TEST--
Class members: object-cast-string
--FILE--
<?php var_dump((object)'foo');
--EXPECT--
object(stdClass)#1 (1) {
  ["scalar"]=>
  string(3) "foo"
}
