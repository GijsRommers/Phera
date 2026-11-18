--TEST--
Static parent magic calls forward the called class
--FILE--
<?php
class ParentCaller { static function __callStatic($n, $a) { echo __CLASS__, "/", static::class; } }
class ChildCaller extends ParentCaller { static function test() { parent::missing(); } }
ChildCaller::test();
--EXPECT--
ParentCaller/ChildCaller
