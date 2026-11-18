--TEST--
__set_state declaration checks named return
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __set_state(array $properties): MagicDeclaration {}
}
--EXPECT--
executed
