--TEST--
Class conformance: protected-static-error
--FILE--
<?php class A {protected static function m(){}} A::m();
--EXPECTF--

Fatal error: Uncaught Error: Call to protected method A::m() from global scope in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
