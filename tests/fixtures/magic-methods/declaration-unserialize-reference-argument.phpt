--TEST--
__unserialize declaration checks reference argument
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __unserialize(array &$properties) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__unserialize() cannot take arguments by reference in %s on line 4
