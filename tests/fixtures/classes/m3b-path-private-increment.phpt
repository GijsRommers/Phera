--TEST--
Class members: path-private-increment
--FILE--
<?php class Foo { private $x=1; function run(){$this->x++; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x":"Foo":private]=>
  int(2)
}
