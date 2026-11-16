--TEST--
modulo_zero
--FILE--
<?php
echo 1%0;
--EXPECTF--

Fatal error: Uncaught DivisionByZeroError: Modulo by zero in %s:2
Stack trace:
#0 {main}
  thrown in %s on line 2
