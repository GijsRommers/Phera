--TEST--
__unserialize declaration checks return type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __unserialize(array $properties): int {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__unserialize(): Return type must be void when declared in %s on line 4
