--TEST--
__wakeup declaration checks accepted
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __wakeup(): void {}
}
--EXPECT--
executed
