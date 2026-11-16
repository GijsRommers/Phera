--TEST--
Class members: path-protected-compound
--FILE--
<?php class Foo { protected $x=1; function run(){$this->x+=3; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x":protected]=>
  int(4)
}
