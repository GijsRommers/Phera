--TEST--
__callStatic declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __callStatic(string $name, array $args) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__callStatic() must be static in %s on line 4
