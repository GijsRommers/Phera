--TEST--
Class conformance: type-error-argument-string
--FILE--
<?php class P {} function take(P $value) {} take('bad');
--EXPECTF--

Fatal error: Uncaught TypeError: take(): Argument #1 ($value) must be of type P, string given, called in %s on line 1 and defined in %s:1
Stack trace:
#0 %s(1): take('bad')
#1 {main}
  thrown in %s on line 1
