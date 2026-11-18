--TEST--
__unserialize declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected function __unserialize(array $properties) {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__unserialize() must have public visibility in %s on line 4
executed
