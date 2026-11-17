--TEST--
Class members: path-protected-append
--FILE--
<?php class Foo { protected $x=1; function run(){$this->x=[]; $this->x[]=4; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x":protected]=>
  array(1) {
    [0]=>
    int(4)
  }
}
