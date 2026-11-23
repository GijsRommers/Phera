--TEST--
__set_state declaration checks parameter type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __set_state(int $properties) {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__set_state(): Parameter #1 ($properties) must be of type array when declared in %s on line 4
