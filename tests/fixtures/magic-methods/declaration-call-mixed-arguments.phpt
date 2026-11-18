--TEST--
__call declaration checks mixed arguments
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __call(mixed $name, mixed $args) {}
}
--EXPECT--
executed
