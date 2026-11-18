--TEST--
__toString declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __toString($extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__toString() cannot take arguments in %s on line 4
