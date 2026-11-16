--TEST--
Class members: layout-private
--FILE--
<?php class A {private $x=1;} class B extends A {private $x=2;} var_dump(new B);
--EXPECT--
object(B)#1 (2) {
  ["x":"A":private]=>
  int(1)
  ["x":"B":private]=>
  int(2)
}
