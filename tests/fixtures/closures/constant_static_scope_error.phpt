--TEST--
constant static scope error
--FILE--
<?php
class A{const C=static::f(...);static function f(){}}
--EXPECTF--

Fatal error: "static" is not allowed in compile-time constants in %s on line 2
