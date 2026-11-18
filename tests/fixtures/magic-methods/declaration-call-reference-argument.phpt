--TEST--
__call declaration checks reference argument
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __call(string &$name, array $args) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__call() cannot take arguments by reference in %s on line 4
