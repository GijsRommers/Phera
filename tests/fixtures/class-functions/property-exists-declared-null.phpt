--TEST--
Property exists declared null
--FILE--
<?php
class A { public $p; } var_dump(property_exists(A::class, "p"));
--EXPECT--
bool(true)
