--TEST--
Get object vars inside class
--FILE--
<?php
class A { private $hidden=1; function f() { return get_object_vars($this); } } var_dump((new A)->f());
--EXPECT--
array(1) {
  ["hidden"]=>
  int(1)
}
