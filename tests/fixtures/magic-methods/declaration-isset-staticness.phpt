--TEST--
__isset declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __isset(string $name) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__isset() cannot be static in %s on line 4
