--TEST--
__set declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected function __set(string $name, mixed $value) {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__set() must have public visibility in %s on line 4
executed
