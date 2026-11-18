--TEST--
__clone declaration checks variadic
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __clone(...$extra) {}
}
--EXPECT--
executed
