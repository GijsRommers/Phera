--TEST--
__sleep declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __sleep() {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__sleep() cannot be static in %s on line 4
