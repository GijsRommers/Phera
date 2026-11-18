--TEST--
__toString declaration checks never return
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __toString(): never {}
}
--EXPECT--
executed
