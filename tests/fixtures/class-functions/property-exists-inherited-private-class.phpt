--TEST--
Property exists inherited private class
--FILE--
<?php
class A { private $p=1; } class B extends A {} var_dump(property_exists(B::class, "p"));
--EXPECT--
bool(false)
