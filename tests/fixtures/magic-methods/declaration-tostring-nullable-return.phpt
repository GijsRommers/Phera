--TEST--
__toString declaration checks nullable return
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __toString(): ?string {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__toString(): Return type must be string when declared in %s on line 4
