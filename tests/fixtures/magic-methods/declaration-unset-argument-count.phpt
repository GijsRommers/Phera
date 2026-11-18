--TEST--
__unset declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __unset(string $name, $extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__unset() must take exactly 1 argument in %s on line 4
