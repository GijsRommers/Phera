--TEST--
__unserialize declaration checks accepted
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __unserialize(array $data): void {}
}
--EXPECT--
executed
