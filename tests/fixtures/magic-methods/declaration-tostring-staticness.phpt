--TEST--
__toString declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __toString() {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__toString() cannot be static in %s on line 4
