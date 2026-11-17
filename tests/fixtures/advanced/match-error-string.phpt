--TEST--
match-error-string
--FILE--
<?php match('foo'){};

--EXPECTF--

Fatal error: Uncaught UnhandledMatchError: Unhandled match case 'foo' in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
