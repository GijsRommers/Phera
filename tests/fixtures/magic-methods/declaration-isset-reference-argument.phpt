--TEST--
__isset declaration checks reference argument
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __isset(string &$name) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__isset() cannot take arguments by reference in %s on line 4
