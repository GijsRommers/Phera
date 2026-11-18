--TEST--
__toString declaration checks return type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __toString(): int {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__toString(): Return type must be string when declared in %s on line 4
