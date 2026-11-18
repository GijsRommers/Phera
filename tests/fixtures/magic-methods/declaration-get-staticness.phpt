--TEST--
__get declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __get(string $name) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__get() cannot be static in %s on line 4
