--TEST--
__set_state declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected static function __set_state(array $properties) {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__set_state() must have public visibility in %s on line 4
executed
