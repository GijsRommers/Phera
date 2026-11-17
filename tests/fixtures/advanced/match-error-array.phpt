--TEST--
match-error-array
--FILE--
<?php match([]){};

--EXPECTF--

Fatal error: Uncaught UnhandledMatchError: Unhandled match case of type array in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
