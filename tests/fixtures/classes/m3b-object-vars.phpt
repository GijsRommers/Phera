--TEST--
Class members: object-vars
--FILE--
<?php class Foo {public $x=1;private $y=2;} var_dump(get_object_vars(new Foo));
--EXPECT--
array(1) {
  ["x"]=>
  int(1)
}
