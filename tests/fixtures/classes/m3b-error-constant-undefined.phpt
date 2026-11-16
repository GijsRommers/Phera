--TEST--
Class members: error-constant-undefined
--FILE--
<?php class Foo {} var_dump(Foo::A);
--EXPECTF--

Fatal error: Uncaught Error: Undefined constant Foo::A in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
