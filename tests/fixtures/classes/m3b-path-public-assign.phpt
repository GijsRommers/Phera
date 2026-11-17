--TEST--
Class members: path-public-assign
--FILE--
<?php class Foo { public $x=1; function run(){$this->x=7; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x"]=>
  int(7)
}
