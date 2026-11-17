--TEST--
Class conformance: type-error-argument-object
--FILE--
<?php class P {} function take(P $value) {} take(new stdClass);
--EXPECTF--

Fatal error: Uncaught TypeError: take(): Argument #1 ($value) must be of type P, stdClass given, called in %s on line 1 and defined in %s:1
Stack trace:
#0 %s(1): take(Object(stdClass))
#1 {main}
  thrown in %s on line 1
