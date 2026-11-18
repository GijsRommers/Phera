--TEST--
traits: static local independent in alias
--FILE--
<?php
trait T {static function m(){static $n=0;return ++$n;}} class A{use T {m as n;}} echo A::m(),A::n(),A::m(),A::n();
--EXPECT--
1122
