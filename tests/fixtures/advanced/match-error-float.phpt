--TEST--
match-error-float
--FILE--
<?php match(5.0){};

--EXPECTF--

Fatal error: Uncaught UnhandledMatchError: Unhandled match case 5.0 in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
