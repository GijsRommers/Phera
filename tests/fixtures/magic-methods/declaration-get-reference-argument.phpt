--TEST--
__get declaration checks reference argument
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __get(string &$name) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__get() cannot take arguments by reference in %s on line 4
