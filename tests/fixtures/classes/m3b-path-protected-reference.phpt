--TEST--
Class members: path-protected-reference
--FILE--
<?php class Foo { protected $x=1; function run(){$ref=&$this->x; $ref=9; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x":protected]=>
  &int(9)
}
