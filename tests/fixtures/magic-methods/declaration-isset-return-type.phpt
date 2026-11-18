--TEST--
__isset declaration checks return type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __isset(string $name): int {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__isset(): Return type must be bool when declared in %s on line 4
