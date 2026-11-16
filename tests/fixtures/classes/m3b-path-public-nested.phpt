--TEST--
Class members: path-public-nested
--FILE--
<?php class Foo { public $x=1; function run(){$this->x=[]; $this->x['k'][]=8; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x"]=>
  array(1) {
    ["k"]=>
    array(1) {
      [0]=>
      int(8)
    }
  }
}
