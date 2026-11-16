--TEST--
gap-match-bool
--FILE--
<?php match(true){};
--EXPECTF--

Fatal error: Uncaught UnhandledMatchError: Unhandled match case true in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
