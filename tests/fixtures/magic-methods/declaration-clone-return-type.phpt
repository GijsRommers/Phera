--TEST--
__clone declaration checks return type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __clone(): int {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__clone(): Return type must be void when declared in %s on line 4
