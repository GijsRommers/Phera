--TEST--
Class members: path-private-assign
--FILE--
<?php class Foo { private $x=1; function run(){$this->x=7; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x":"Foo":private]=>
  int(7)
}
