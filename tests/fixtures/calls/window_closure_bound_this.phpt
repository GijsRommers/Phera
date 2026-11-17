--TEST--
Call window: closure bound this
--FILE--
<?php
class A{public $x=7;function f(){return function($a){return $this->x+$a;};}}$f=(new A)->f();var_dump(array_map($f,[1,2]));
--EXPECT--
array(2) {
  [0]=>
  int(8)
  [1]=>
  int(9)
}
