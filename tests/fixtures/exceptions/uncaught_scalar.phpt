--TEST--
Exceptions: uncaught_scalar
--FILE--
<?php
throw 17;
--EXPECTF--

Fatal error: Uncaught Error: Can only throw objects in %s:2
Stack trace:
#0 {main}
  thrown in %s on line 2
