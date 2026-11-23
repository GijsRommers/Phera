--TEST--
__isset declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected function __isset(string $name) {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__isset() must have public visibility in %s on line 4
executed
