--TEST--
__unserialize declaration checks parameter type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __unserialize(int $properties) {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__unserialize(): Parameter #1 ($properties) must be of type array when declared in %s on line 4
