--TEST--
Class members: path-protected-nested
--FILE--
<?php class Foo { protected $x=1; function run(){$this->x=[]; $this->x['k'][]=8; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x":protected]=>
  array(1) {
    ["k"]=>
    array(1) {
      [0]=>
      int(8)
    }
  }
}
