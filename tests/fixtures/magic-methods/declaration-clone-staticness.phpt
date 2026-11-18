--TEST--
__clone declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __clone() {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__clone() cannot be static in %s on line 4
