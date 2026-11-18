--TEST--
__wakeup declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __wakeup($extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__wakeup() cannot take arguments in %s on line 4
