--TEST--
__unserialize declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __unserialize(array $properties) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__unserialize() cannot be static in %s on line 4
