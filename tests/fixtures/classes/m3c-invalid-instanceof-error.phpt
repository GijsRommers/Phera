--TEST--
Class conformance: invalid-instanceof-error
--FILE--
<?php $target=3; var_dump((new stdClass) instanceof $target);
--EXPECTF--

Fatal error: Uncaught Error: Class name must be a valid object or a string in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
