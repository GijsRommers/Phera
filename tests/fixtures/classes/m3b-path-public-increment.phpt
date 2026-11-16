--TEST--
Class members: path-public-increment
--FILE--
<?php class Foo { public $x=1; function run(){$this->x++; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x"]=>
  int(2)
}
