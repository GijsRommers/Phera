--TEST--
__invoke declaration checks variadic reference
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __invoke(&...$args) {}
}
--EXPECT--
executed
