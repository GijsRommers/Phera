--TEST--
Writing an initially null property converts its array key once
--FILE--
<?php
class Box { public $any; }
$b = new Box;
$b->any[1.5] = 1;
var_dump($b->any);
--EXPECTF--
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 4
array(1) {
  [1]=>
  int(1)
}
