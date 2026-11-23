--TEST--
__unset declaration checks reference argument
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __unset(string &$name) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__unset() cannot take arguments by reference in %s on line 4
