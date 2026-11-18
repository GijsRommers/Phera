--TEST--
Alias in inherited signature
--FILE--
<?php
class Target {}
class Base { public function f(Target $value) {} }
class_alias(Target::class, "Alias");
class Derived extends Base { public function f(Alias $value) {} }
var_dump(new Derived instanceof Base);
--EXPECT--
bool(true)
