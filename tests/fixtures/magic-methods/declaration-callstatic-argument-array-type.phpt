--TEST--
__callStatic declaration checks argument array type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __callStatic($name, string $args) {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__callStatic(): Parameter #2 ($args) must be of type array when declared in %s on line 4
