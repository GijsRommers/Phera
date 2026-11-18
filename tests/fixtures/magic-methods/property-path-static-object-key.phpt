--TEST--
A property write reached through a static array property uses one converted key
--FILE--
<?php
class Box {
    public static array $instances = [];
    public array $data = [];
}
$b = new Box;
Box::$instances[] = $b;
Box::$instances[0]->data[1.5] = 1;
var_dump($b->data);
--EXPECTF--
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 8
array(1) {
  [1]=>
  int(1)
}
