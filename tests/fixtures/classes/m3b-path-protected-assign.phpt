--TEST--
Class members: path-protected-assign
--FILE--
<?php class Foo { protected $x=1; function run(){$this->x=7; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x":protected]=>
  int(7)
}
