--TEST--
Class conformance: self-static
--FILE--
<?php class D {static function m(){return 1;} function g(){return self::m()+static::m();}} echo (new D)->g();
--EXPECT--
2
