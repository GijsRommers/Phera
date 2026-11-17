--TEST--
Exceptions: uncaught_object
--FILE--
<?php
throw new Exception("message", 17);
--EXPECTF--

Fatal error: Uncaught Exception: message in %s:2
Stack trace:
#0 {main}
  thrown in %s on line 2
