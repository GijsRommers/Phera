--TEST--
__get declaration checks variadic tail
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __get($name, ...$extra) {}
}
--EXPECT--
executed
