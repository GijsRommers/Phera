--TEST--
Class members: path-private-isset
--FILE--
<?php class Foo { private $x=1; function run(){var_dump(isset($this->x)); var_dump($this);} } (new Foo)->run();
--EXPECT--
bool(true)
object(Foo)#1 (1) {
  ["x":"Foo":private]=>
  int(1)
}
