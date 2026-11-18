--TEST--
__set_state declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __set_state(array $properties) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__set_state() must be static in %s on line 4
