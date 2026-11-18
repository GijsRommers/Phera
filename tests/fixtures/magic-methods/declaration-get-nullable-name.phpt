--TEST--
__get declaration checks nullable name
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __get(?string $name) {}
}
--EXPECT--
executed
