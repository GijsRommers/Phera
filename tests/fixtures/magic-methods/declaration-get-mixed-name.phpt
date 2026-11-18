--TEST--
__get declaration checks mixed name
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __get(mixed $name) {}
}
--EXPECT--
executed
