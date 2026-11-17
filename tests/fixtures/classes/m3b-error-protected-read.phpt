--TEST--
Class members: error-protected-read
--FILE--
<?php class Foo {private $x; protected $y; public int $z; public readonly int $r;} $a=new Foo; var_dump($a->y);
--EXPECTF--

Fatal error: Uncaught Error: Cannot access protected property Foo::$y in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
