--TEST--
Autoloads inheritance signature types
--FILE--
<?php
class A { function f(BaseType $param) {} }
spl_autoload_register(function($name) { echo $name, "\n"; class_alias(stdClass::class, $name); });
echo "before\n";
class B extends A { function f(AliasType $param) {} }
echo "after\n";
--EXPECT--
before
BaseType
AliasType
after
