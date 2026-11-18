--TEST--
__set declaration checks parameter type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __set(int $name, mixed $value) {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__set(): Parameter #1 ($name) must be of type string when declared in %s on line 4
