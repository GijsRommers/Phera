--TEST--
traits: class magic constant
--FILE--
<?php
trait T{static function m(){return __CLASS__;}} class C{use T;} class D extends C{} echo D::m();
--EXPECT--
C
