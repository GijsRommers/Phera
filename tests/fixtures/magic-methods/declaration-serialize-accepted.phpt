--TEST--
__serialize declaration checks accepted
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __serialize(): array {}
}
--EXPECT--
executed
