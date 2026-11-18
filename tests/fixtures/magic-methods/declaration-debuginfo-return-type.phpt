--TEST--
__debugInfo declaration checks return type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __debugInfo(): int {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__debugInfo(): Return type must be ?array when declared in %s on line 4
