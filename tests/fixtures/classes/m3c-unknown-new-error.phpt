--TEST--
Class conformance: unknown-new-error
--FILE--
<?php $name='Missing';new $name;
--EXPECTF--

Fatal error: Uncaught Error: Class "Missing" not found in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
