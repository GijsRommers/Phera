--TEST--
Call window: default class expression
--FILE--
<?php
class A{const X=3;static function f($x=self::X+2){echo $x;}}A::f();A::f(8);
--EXPECT--
58
