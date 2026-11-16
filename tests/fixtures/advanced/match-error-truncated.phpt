--TEST--
match-error-truncated
--FILE--
<?php match('abcdefghijklmnopq'){};

--EXPECTF--

Fatal error: Uncaught UnhandledMatchError: Unhandled match case 'abcdefghijklmno...' in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
