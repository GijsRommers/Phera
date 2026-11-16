--TEST--
unsupported_add
--FILE--
<?php
echo []+1;
--EXPECTF--

Fatal error: Uncaught TypeError: Unsupported operand types: array + int in %s:2
Stack trace:
#0 {main}
  thrown in %s on line 2
