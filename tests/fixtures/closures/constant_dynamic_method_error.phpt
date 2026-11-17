--TEST--
constant dynamic method error
--FILE--
<?php
class A{static function f(){}}const C=A::$m(...);
--EXPECTF--

Fatal error: Cannot use dynamic method name in constant expression in %s on line 2
