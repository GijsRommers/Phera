--TEST--
Class members: layout-inherited
--FILE--
<?php class A {public $x=1;protected $y=2;} class B extends A {public $z=3;} var_dump(new B);
--EXPECT--
object(B)#1 (3) {
  ["x"]=>
  int(1)
  ["y":protected]=>
  int(2)
  ["z"]=>
  int(3)
}
