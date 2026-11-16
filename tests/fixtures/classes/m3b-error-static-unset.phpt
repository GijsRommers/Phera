--TEST--
Class members: error-static-unset
--FILE--
<?php class Foo {public static $x;} unset(Foo::$x);
--EXPECTF--

Fatal error: Uncaught Error: Attempt to unset static property Foo::$x in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
