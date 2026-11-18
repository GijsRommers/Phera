--TEST--
__get declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected function __get(string $name) {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__get() must have public visibility in %s on line 4
executed
