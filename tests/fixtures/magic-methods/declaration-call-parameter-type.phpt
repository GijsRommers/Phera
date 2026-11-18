--TEST--
__call declaration checks parameter type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __call(int $name, array $args) {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__call(): Parameter #1 ($name) must be of type string when declared in %s on line 4
