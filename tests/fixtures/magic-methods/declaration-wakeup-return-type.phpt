--TEST--
__wakeup declaration checks return type
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __wakeup(): int {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__wakeup(): Return type must be void when declared in %s on line 4
