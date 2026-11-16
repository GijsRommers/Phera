--TEST--
Class members: static-typed-uninitialized
--FILE--
<?php class Foo {public static int $x;} var_dump(Foo::$x);
--EXPECTF--

Fatal error: Uncaught Error: Typed static property Foo::$x must not be accessed before initialization in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
