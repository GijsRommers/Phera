--TEST--
This in class
--FILE--
<?php
class A { private $x=9; function f() { return eval('return $this->x;'); } } var_dump((new A)->f());
--EXPECT--
int(9)
