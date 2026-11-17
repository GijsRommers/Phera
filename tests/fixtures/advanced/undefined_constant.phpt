--TEST--
undefined_constant
--FILE--
<?php
echo ABSENT;
--EXPECTF--

Fatal error: Uncaught Error: Undefined constant "ABSENT" in %s:2
Stack trace:
#0 {main}
  thrown in %s on line 2
