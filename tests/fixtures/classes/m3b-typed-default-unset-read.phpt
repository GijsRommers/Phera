--TEST--
Class members: typed-default-unset-read
--FILE--
<?php class Foo {public int $x=1;} $a=new Foo; unset($a->x); var_dump($a->x);
--EXPECTF--

Fatal error: Uncaught Error: Typed property Foo::$x must not be accessed before initialization in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
