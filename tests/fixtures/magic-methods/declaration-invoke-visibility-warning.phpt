--TEST--
__invoke declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected function __invoke() {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__invoke() must have public visibility in %s on line 4
executed
