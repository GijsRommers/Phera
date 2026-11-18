--TEST--
traits: compatible property default relative constant
--FILE--
<?php
trait T {public $x=self::X;} trait U{public $x=self::X;} class C{use T,U;const X=42;} echo (new C)->x;
--EXPECT--
42
