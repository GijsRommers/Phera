--TEST--
Class members: error-private-read
--FILE--
<?php class Foo {private $x; protected $y; public int $z; public readonly int $r;} $a=new Foo; var_dump($a->x);
--EXPECTF--

Fatal error: Uncaught Error: Cannot access private property Foo::$x in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
