--TEST--
__serialize declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __serialize() {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__serialize() cannot be static in %s on line 4
