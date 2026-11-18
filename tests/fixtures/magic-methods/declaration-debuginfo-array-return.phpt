--TEST--
__debugInfo declaration checks array return
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __debugInfo(): array {}
}
--EXPECT--
executed
