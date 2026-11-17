--TEST--
Class members: error-dynamic-deprecated
--FILE--
<?php class Foo {private $x; protected $y; public int $z; public readonly int $r;} $a=new Foo; $a->missing=7;
--EXPECTF--

Deprecated: Creation of dynamic property Foo::$missing is deprecated in %s on line 1
