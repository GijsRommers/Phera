--TEST--
The parent getter and setter syntax supports plain backed properties
--FILE--
<?php
class Base { public int $x = 1; }
class Child extends Base {
    public int $x {
        get => parent::$x::get() + 2;
        set { parent::$x::set($value * 3); }
    }
}
$c = new Child;
$c->x = 4;
var_dump($c->x, (array) $c);
--EXPECT--
int(14)
array(1) {
  ["x"]=>
  int(12)
}
