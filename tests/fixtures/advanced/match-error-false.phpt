--TEST--
match-error-false
--FILE--
<?php match(false){};

--EXPECTF--

Fatal error: Uncaught UnhandledMatchError: Unhandled match case false in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
