--TEST--
match-error-escaped
--FILE--
<?php match("a\nb\tc\0\x80"){};

--EXPECTF--

Fatal error: Uncaught UnhandledMatchError: Unhandled match case 'a\nb\tc\x00\x80' in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
