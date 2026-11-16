--TEST--
Class conformance: nonstatic-error
--FILE--
<?php class A {function m(){}} A::m();
--EXPECTF--

Fatal error: Uncaught Error: Non-static method A::m() cannot be called statically in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
