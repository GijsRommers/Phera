--TEST--
traits: static local retained in subclass
--FILE--
<?php
trait T {static function m(){static $n=0;return ++$n;}} class A{use T;} class B extends A{} echo A::m(),B::m();
--EXPECT--
12
