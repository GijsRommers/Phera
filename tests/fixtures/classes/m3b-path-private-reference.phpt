--TEST--
Class members: path-private-reference
--FILE--
<?php class Foo { private $x=1; function run(){$ref=&$this->x; $ref=9; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x":"Foo":private]=>
  &int(9)
}
