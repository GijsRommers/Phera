--TEST--
Class members: path-public-reference
--FILE--
<?php class Foo { public $x=1; function run(){$ref=&$this->x; $ref=9; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x"]=>
  &int(9)
}
