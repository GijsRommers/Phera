--TEST--
__serialize declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected function __serialize() {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__serialize() must have public visibility in %s on line 4
executed
