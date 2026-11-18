--TEST--
__call declaration checks argument array type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __call($name, string $args) {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__call(): Parameter #2 ($args) must be of type array when declared in %s on line 4
