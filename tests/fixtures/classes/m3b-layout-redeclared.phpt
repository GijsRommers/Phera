--TEST--
Class members: layout-redeclared
--FILE--
<?php class A {public $x=1;public $y=2;} class B extends A {public $x=9;} var_dump(new B);
--EXPECT--
object(B)#1 (2) {
  ["x"]=>
  int(9)
  ["y"]=>
  int(2)
}
