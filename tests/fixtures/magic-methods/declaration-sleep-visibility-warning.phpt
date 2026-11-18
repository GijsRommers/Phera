--TEST--
__sleep declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected function __sleep() {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__sleep() must have public visibility in %s on line 4
executed
