--TEST--
__set declaration checks return type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __set(string $name, mixed $value): int {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__set(): Return type must be void when declared in %s on line 4
