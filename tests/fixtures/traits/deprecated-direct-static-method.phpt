--TEST--
Direct static trait method calls warn even through a cached call site
--FILE--
<?php
trait Behavior { public static function test(): string { return __CLASS__; } }
for ($i = 0; $i < 2; ++$i) { var_dump(Behavior::test()); }
--EXPECTF--
Deprecated: Calling static trait method Behavior::test is deprecated, it should only be called on a class using the trait in %s on line 3
string(8) "Behavior"

Deprecated: Calling static trait method Behavior::test is deprecated, it should only be called on a class using the trait in %s on line 3
string(8) "Behavior"
