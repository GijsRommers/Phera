--TEST--
A by-reference getter runs before an overloaded property array key is converted
--FILE--
<?php
class Box {
    public array $data = [];
    public function &__get($name) { echo "get $name\n"; return $this->data; }
}
$b = new Box;
$b->alias[1.5] += 2;
$r =& $b->alias[2.5];
$r = 4;
var_dump($b->data);
--EXPECTF--
get alias

Deprecated: Implicit conversion from float 1.5 to int loses precision in /private%s on line 7

Warning: Undefined array key 1 in /private%s on line 7
get alias

Deprecated: Implicit conversion from float 2.5 to int loses precision in /private%s on line 8
array(2) {
  [1]=>
  int(2)
  [2]=>
  &int(4)
}
