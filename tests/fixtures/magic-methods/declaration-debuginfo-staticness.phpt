--TEST--
__debugInfo declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __debugInfo() {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__debugInfo() cannot be static in %s on line 4
