--TEST--
Class members: path-public-append
--FILE--
<?php class Foo { public $x=1; function run(){$this->x=[]; $this->x[]=4; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x"]=>
  array(1) {
    [0]=>
    int(4)
  }
}
