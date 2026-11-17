--TEST--
Class members: path-protected-empty
--FILE--
<?php class Foo { protected $x=1; function run(){var_dump(empty($this->x)); var_dump($this);} } (new Foo)->run();
--EXPECT--
bool(false)
object(Foo)#1 (1) {
  ["x":protected]=>
  int(1)
}
