--TEST--
__set_state declaration checks reference argument
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __set_state(array &$properties) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__set_state() cannot take arguments by reference in %s on line 4
