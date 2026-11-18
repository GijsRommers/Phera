--TEST--
__clone declaration checks private
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    private function __clone() {}
}
--EXPECT--
executed
