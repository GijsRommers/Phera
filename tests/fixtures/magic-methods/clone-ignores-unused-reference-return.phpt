--TEST--
Reading a by-reference getter by value does not keep an alias across cloning
--FILE--
<?php
#[AllowDynamicProperties]
class Holder { function &getValue() { return $this->value; } }
$o = new Holder;
$o->value = [1];
$value = $o->getValue();
$c = clone $o;
$c->value = [];
var_dump($o->value);
--EXPECT--
array(1) {
  [0]=>
  int(1)
}
