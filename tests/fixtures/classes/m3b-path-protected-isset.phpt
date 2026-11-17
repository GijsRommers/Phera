--TEST--
Class members: path-protected-isset
--FILE--
<?php class Foo { protected $x=1; function run(){var_dump(isset($this->x)); var_dump($this);} } (new Foo)->run();
--EXPECT--
bool(true)
object(Foo)#1 (1) {
  ["x":protected]=>
  int(1)
}
