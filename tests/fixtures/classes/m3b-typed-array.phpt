--TEST--
Class members: typed-array
--FILE--
<?php class Foo {public array $x;} $a=new Foo; $a->x=[1]; var_dump($a->x);
--EXPECT--
array(1) {
  [0]=>
  int(1)
}
