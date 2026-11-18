--TEST--
__serialize declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __serialize($extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__serialize() cannot take arguments in %s on line 4
