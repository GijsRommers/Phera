--TEST--
__invoke declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __invoke() {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__invoke() cannot be static in %s on line 4
