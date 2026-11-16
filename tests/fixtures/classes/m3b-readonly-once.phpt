--TEST--
Class members: readonly-once
--FILE--
<?php class Foo {public readonly int $x; function init(){$this->x=7;}} $a=new Foo; $a->init(); var_dump($a);
--EXPECT--
object(Foo)#1 (1) {
  ["x"]=>
  int(7)
}
