--TEST--
__get declaration checks variadic alone
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __get(...$name) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__get() must take exactly 1 argument in %s on line 4
