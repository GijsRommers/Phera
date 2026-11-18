--TEST--
traits: static local per class
--FILE--
<?php
trait T {static function m(){static $n=0;return ++$n;}} class A{use T;} class B{use T;} echo A::m(),B::m(),A::m(),B::m();
--EXPECT--
1122
