--TEST--
__unset declaration checks return type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __unset(string $name): int {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__unset(): Return type must be void when declared in %s on line 4
