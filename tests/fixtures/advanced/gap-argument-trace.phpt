--TEST--
gap-argument-trace
--FILE--
<?php function f(int $x){}f([]);
--EXPECTF--

Fatal error: Uncaught TypeError: f(): Argument #1 ($x) must be of type int, array given, called in %s on line 1 and defined in %s:1
Stack trace:
#0 %s(1): f(Array)
#1 {main}
  thrown in %s on line 1
