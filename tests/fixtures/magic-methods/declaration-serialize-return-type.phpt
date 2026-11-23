--TEST--
__serialize declaration checks return type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __serialize(): int {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__serialize(): Return type must be array when declared in %s on line 4
