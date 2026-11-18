--TEST--
__unset declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __unset(string $name) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__unset() cannot be static in %s on line 4
