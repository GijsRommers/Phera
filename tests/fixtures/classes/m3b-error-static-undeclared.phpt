--TEST--
Class members: error-static-undeclared
--FILE--
<?php class Foo {} var_dump(Foo::$x);
--EXPECTF--

Fatal error: Uncaught Error: Access to undeclared static property Foo::$x in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
