--TEST--
traits: self method
--FILE--
<?php
trait T{function m(){return self::n();}} class C{use T; static function n(){return "C";}} echo (new C)->m();
--EXPECT--
C
