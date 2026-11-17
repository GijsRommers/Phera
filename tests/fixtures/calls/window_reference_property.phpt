--TEST--
Call window: reference property
--FILE--
<?php
class A{public $x=3;}function f(&$x){$x*=2;}$a=new A;f($a->x);var_dump($a->x);
--EXPECT--
int(6)
