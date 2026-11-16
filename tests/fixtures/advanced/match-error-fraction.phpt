--TEST--
match-error-fraction
--FILE--
<?php match(5.5){};

--EXPECTF--

Fatal error: Uncaught UnhandledMatchError: Unhandled match case 5.5 in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
