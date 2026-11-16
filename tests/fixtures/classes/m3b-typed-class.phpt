--TEST--
Class members: typed-class
--FILE--
<?php class A {} class B extends A {} class Foo {public A $x;} $a=new Foo; $a->x=new B; var_dump($a);
--EXPECT--
object(Foo)#1 (1) {
  ["x"]=>
  object(B)#2 (0) {
  }
}
