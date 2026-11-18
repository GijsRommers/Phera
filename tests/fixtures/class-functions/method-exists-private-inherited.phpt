--TEST--
Method exists private inherited
--FILE--
<?php
class A { private function run() {} } class B extends A {} var_dump(method_exists(B::class, "run"), method_exists(new B, "run"));
--EXPECT--
bool(false)
bool(true)
