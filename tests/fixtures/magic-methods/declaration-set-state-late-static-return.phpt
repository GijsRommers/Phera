--TEST--
__set_state declaration checks late static return
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __set_state(array $properties): static {}
}
--EXPECT--
executed
