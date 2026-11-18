--TEST--
__get declaration checks visibility before type error
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    private function __get(int $name) {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__get() must have public visibility in %s on line 4

Fatal error: MagicDeclaration::__get(): Parameter #1 ($name) must be of type string when declared in %s on line 4
