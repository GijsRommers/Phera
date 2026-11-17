--TEST--
Class members: default-protected-array
--FILE--
<?php class Foo { protected $x=['k'=>[1,2]]; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
array(1) {
  ["k"]=>
  array(2) {
    [0]=>
    int(1)
    [1]=>
    int(2)
  }
}
