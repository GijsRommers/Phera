--TEST--
__set declaration checks value type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __set($name, int $value) {}
}
--EXPECT--
executed
