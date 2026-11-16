--TEST--
Class members: path-public-empty
--FILE--
<?php class Foo { public $x=1; function run(){var_dump(empty($this->x)); var_dump($this);} } (new Foo)->run();
--EXPECT--
bool(false)
object(Foo)#1 (1) {
  ["x"]=>
  int(1)
}
