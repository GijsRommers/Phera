--TEST--
traits: self class
--FILE--
<?php
trait T{static function m(){return self::class;}} class C{use T;} class D extends C{} echo D::m();
--EXPECT--
C
