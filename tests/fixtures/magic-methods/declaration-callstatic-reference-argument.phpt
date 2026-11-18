--TEST--
__callStatic declaration checks reference argument
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __callStatic(string &$name, array $args) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__callStatic() cannot take arguments by reference in %s on line 4
