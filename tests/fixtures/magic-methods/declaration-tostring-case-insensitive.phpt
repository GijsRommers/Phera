--TEST--
__toString declaration checks case insensitive
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __TOSTRING($extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__TOSTRING() cannot take arguments in %s on line 4
