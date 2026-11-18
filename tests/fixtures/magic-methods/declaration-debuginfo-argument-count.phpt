--TEST--
__debugInfo declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __debugInfo($extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__debugInfo() cannot take arguments in %s on line 4
