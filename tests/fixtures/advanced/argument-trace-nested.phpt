--TEST--
argument-trace-nested
--FILE--
<?php
function leaf(int $value) {}
function outer($name, $flag, $count) { leaf([]); }
outer('abcdefghijklmnopqr', false, 5.0);

--EXPECTF--

Fatal error: Uncaught TypeError: leaf(): Argument #1 ($value) must be of type int, array given, called in %s on line 3 and defined in %s:2
Stack trace:
#0 %s(3): leaf(Array)
#1 %s(4): outer('abcdefghijklmno...', false, 5.0)
#2 {main}
  thrown in %s on line 2
