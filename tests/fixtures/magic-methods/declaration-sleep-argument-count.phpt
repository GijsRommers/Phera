--TEST--
__sleep declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __sleep($extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__sleep() cannot take arguments in %s on line 4
