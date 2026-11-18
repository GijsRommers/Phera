--TEST--
__clone declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __clone($extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__clone() cannot take arguments in %s on line 4
