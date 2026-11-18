--TEST--
__set_state declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __set_state(array $properties, $extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__set_state() must take exactly 1 argument in %s on line 4
