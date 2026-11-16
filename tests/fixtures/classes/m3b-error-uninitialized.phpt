--TEST--
Class members: error-uninitialized
--FILE--
<?php class Foo {private $x; protected $y; public int $z; public readonly int $r;} $a=new Foo; var_dump($a->z);
--EXPECTF--

Fatal error: Uncaught Error: Typed property Foo::$z must not be accessed before initialization in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
