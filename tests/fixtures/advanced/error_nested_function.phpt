--TEST--
error_nested_function
--FILE--
<?php
function f(){absent();}f();
--EXPECTF--

Fatal error: Uncaught Error: Call to undefined function absent() in %s:2
Stack trace:
#0 %s(2): f()
#1 {main}
  thrown in %s on line 2
