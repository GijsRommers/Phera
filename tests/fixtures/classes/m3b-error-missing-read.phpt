--TEST--
Class members: error-missing-read
--FILE--
<?php class Foo {private $x; protected $y; public int $z; public readonly int $r;} $a=new Foo; var_dump($a->missing);
--EXPECTF--

Warning: Undefined property: Foo::$missing in %s on line 1
NULL
