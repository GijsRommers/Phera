--TEST--
Class members: path-private-empty
--FILE--
<?php class Foo { private $x=1; function run(){var_dump(empty($this->x)); var_dump($this);} } (new Foo)->run();
--EXPECT--
bool(false)
object(Foo)#1 (1) {
  ["x":"Foo":private]=>
  int(1)
}
