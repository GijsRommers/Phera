--TEST--
__set declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __set(string $name, mixed $value) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__set() cannot be static in %s on line 4
