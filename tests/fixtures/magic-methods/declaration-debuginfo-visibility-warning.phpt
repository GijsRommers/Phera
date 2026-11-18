--TEST--
__debugInfo declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected function __debugInfo() {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__debugInfo() must have public visibility in %s on line 4
executed
