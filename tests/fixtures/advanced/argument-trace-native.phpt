--TEST--
argument-trace-native
--FILE--
<?php
function outer($name, $flag) { strlen([]); }
outer('test', true);

--EXPECTF--

Fatal error: Uncaught TypeError: strlen(): Argument #1 ($string) must be of type string, array given in %s:2
Stack trace:
#0 %s(3): outer('test', true)
#1 {main}
  thrown in %s on line 2
