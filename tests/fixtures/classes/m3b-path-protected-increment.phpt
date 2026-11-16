--TEST--
Class members: path-protected-increment
--FILE--
<?php class Foo { protected $x=1; function run(){$this->x++; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x":protected]=>
  int(2)
}
