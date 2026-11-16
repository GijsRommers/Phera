--TEST--
Class members: readonly-class-dynamic
--FILE--
<?php readonly class Foo {} $a=new Foo; $a->x=1;
--EXPECTF--

Fatal error: Uncaught Error: Cannot create dynamic property Foo::$x in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
