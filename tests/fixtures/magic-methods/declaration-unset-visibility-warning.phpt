--TEST--
__unset declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected function __unset(string $name) {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__unset() must have public visibility in %s on line 4
executed
