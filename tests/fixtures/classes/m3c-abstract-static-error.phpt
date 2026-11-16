--TEST--
Class conformance: abstract-static-error
--FILE--
<?php abstract class A {abstract static function m();} A::m();
--EXPECTF--

Fatal error: Uncaught Error: Cannot call abstract method A::m() in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
