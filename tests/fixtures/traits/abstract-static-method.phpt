--TEST--
traits: abstract static method
--FILE--
<?php
trait T{abstract static function m():int;static function run(){return self::m();}} class C{use T;static function m():int{return 5;}} echo C::run();
--EXPECT--
5
