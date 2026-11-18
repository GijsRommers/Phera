--TEST--
__debugInfo declaration checks nullable return
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __debugInfo(): ?array {}
}
--EXPECT--
executed
