--TEST--
A property array write evaluates the key and value once before converting the key
--FILE--
<?php
function key_value() { echo "key evaluated\n"; return 2.5; }
function item_value() { echo "value evaluated\n"; return 'v'; }
class Box { public array $data = []; }
$b = new Box;
$b->data[key_value()] = item_value();
var_dump($b->data);
--EXPECTF--
key evaluated
value evaluated

Deprecated: Implicit conversion from float 2.5 to int loses precision in /private%s on line 6
array(1) {
  [2]=>
  string(1) "v"
}
