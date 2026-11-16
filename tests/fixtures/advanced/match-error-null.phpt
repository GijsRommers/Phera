--TEST--
match-error-null
--FILE--
<?php match(null){};

--EXPECTF--

Fatal error: Uncaught UnhandledMatchError: Unhandled match case NULL in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
