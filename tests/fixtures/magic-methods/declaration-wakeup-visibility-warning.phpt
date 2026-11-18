--TEST--
__wakeup declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected function __wakeup() {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__wakeup() must have public visibility in %s on line 4
executed
