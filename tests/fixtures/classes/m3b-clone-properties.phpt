--TEST--
Class members: clone-properties
--FILE--
<?php class Foo {public $x=[1];} $a=new Foo; $b=clone $a; $b->x[]=2; var_dump($a,$b);
--EXPECT--
object(Foo)#1 (1) {
  ["x"]=>
  array(1) {
    [0]=>
    int(1)
  }
}
object(Foo)#2 (1) {
  ["x"]=>
  array(2) {
    [0]=>
    int(1)
    [1]=>
    int(2)
  }
}
