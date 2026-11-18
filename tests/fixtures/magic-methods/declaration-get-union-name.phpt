--TEST--
__get declaration checks union name
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __get(string|int $name) {}
}
--EXPECT--
executed
