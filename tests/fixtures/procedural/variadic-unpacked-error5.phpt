--TEST--
variadic-unpacked-error5
--FILE--
<?php function typed5(int ...$values) {} typed5(...['one'=>[]]);
--EXPECTF--

Fatal error: Uncaught TypeError: typed5(): Argument #1 must be of type int, array given, called in %s on line 1 and defined in %s:1
Stack trace:
#0 %s(1): typed5(one: Array)
#1 {main}
  thrown in %s on line 1
