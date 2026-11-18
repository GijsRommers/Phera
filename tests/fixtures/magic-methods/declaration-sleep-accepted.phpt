--TEST--
__sleep declaration checks accepted
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __sleep(): array {}
}
--EXPECT--
executed
