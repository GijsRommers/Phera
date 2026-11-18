--TEST--
__set declaration checks reference argument
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __set(string &$name, mixed $value) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__set() cannot take arguments by reference in %s on line 4
