--TEST--
Class members: path-public-isset
--FILE--
<?php class Foo { public $x=1; function run(){var_dump(isset($this->x)); var_dump($this);} } (new Foo)->run();
--EXPECT--
bool(true)
object(Foo)#1 (1) {
  ["x"]=>
  int(1)
}
