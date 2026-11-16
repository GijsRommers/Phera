--TEST--
Class members: error-readonly-global
--FILE--
<?php class Foo {private $x; protected $y; public int $z; public readonly int $r;} $a=new Foo; $a->r=3;
--EXPECTF--

Fatal error: Uncaught Error: Cannot modify protected(set) readonly property Foo::$r from global scope in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
