--TEST--
Class conformance: unknown-static-error
--FILE--
<?php Missing::m();
--EXPECTF--

Fatal error: Uncaught Error: Class "Missing" not found in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
