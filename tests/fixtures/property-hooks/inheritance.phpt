--TEST--
A child inherits the setter while overriding and calling the parent getter
--FILE--
<?php
class Base {
    public int $x = 0 { get => $this->x * 2; set => $value; }
}
class Child extends Base {
    public int $x { get => parent::$x::get() + 1; }
}
$c = new Child;
$c->x = 4;
var_dump($c->x, (array) $c);
--EXPECT--
int(9)
array(1) {
  ["x"]=>
  int(4)
}
