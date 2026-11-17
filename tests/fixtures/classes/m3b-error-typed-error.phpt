--TEST--
Class members: error-typed-error
--FILE--
<?php class Foo {private $x; protected $y; public int $z; public readonly int $r;} $a=new Foo; $a->z="bad";
--EXPECTF--

Fatal error: Uncaught TypeError: Cannot assign string to property Foo::$z of type int in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
