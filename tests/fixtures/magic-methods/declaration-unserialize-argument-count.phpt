--TEST--
__unserialize declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __unserialize(array $properties, $extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__unserialize() must take exactly 1 argument in %s on line 4
