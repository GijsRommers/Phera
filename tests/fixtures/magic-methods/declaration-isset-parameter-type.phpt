--TEST--
__isset declaration checks parameter type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __isset(int $name) {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__isset(): Parameter #1 ($name) must be of type string when declared in %s on line 4
