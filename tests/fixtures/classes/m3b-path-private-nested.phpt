--TEST--
Class members: path-private-nested
--FILE--
<?php class Foo { private $x=1; function run(){$this->x=[]; $this->x['k'][]=8; var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (1) {
  ["x":"Foo":private]=>
  array(1) {
    ["k"]=>
    array(1) {
      [0]=>
      int(8)
    }
  }
}
