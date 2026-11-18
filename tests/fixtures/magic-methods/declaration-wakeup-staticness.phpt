--TEST--
__wakeup declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __wakeup() {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__wakeup() cannot be static in %s on line 4
