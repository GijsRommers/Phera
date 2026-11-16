--TEST--
Class members: path-public-compound
--FILE--
<?php class Foo { public $x=1; function run(){$this->x+=3; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x"]=>
  int(4)
}
