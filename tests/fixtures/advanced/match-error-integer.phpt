--TEST--
match-error-integer
--FILE--
<?php match(42){};

--EXPECTF--

Fatal error: Uncaught UnhandledMatchError: Unhandled match case 42 in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
