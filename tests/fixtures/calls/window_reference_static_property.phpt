--TEST--
Call window: reference static property
--FILE--
<?php
class A{public static $x=3;}function f(&$x){$x++;}f(A::$x);var_dump(A::$x);
--EXPECT--
int(4)
