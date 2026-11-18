--TEST--
traits: static method
--FILE--
<?php
trait T {static function m(){return 11;}} class C{use T;} echo C::m();
--EXPECT--
11
