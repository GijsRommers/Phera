--TEST--
__set declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __set(string $name, mixed $value, $extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__set() must take exactly 2 arguments in %s on line 4
