--TEST--
__callStatic declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected static function __callStatic(string $name, array $args) {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__callStatic() must have public visibility in %s on line 4
executed
