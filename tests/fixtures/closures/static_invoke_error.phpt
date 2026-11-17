--TEST--
Invoke methods cannot be static
--FILE--
<?php
class A { public static function __invoke() {} }
--EXPECTF--
Fatal error: Method A::__invoke() cannot be static in %s on line 2
