--TEST--
traits: late static bound class
--FILE--
<?php
trait T{static function m(){return static::class;}} class C{use T;} class D extends C{} echo D::m();
--EXPECT--
D
