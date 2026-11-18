--TEST--
__sleep declaration checks return type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __sleep(): int {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__sleep(): Return type must be array when declared in %s on line 4
