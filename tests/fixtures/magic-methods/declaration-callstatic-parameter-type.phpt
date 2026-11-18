--TEST--
__callStatic declaration checks parameter type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __callStatic(int $name, array $args) {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__callStatic(): Parameter #1 ($name) must be of type string when declared in %s on line 4
