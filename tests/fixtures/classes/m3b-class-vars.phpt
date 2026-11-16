--TEST--
Class members: class-vars
--FILE--
<?php class Foo {public $x=1;public static $y=2;} var_dump(get_class_vars("Foo"));
--EXPECT--
array(2) {
  ["x"]=>
  int(1)
  ["y"]=>
  int(2)
}
