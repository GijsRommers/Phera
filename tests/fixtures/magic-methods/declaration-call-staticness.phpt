--TEST--
__call declaration checks staticness
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __call(string $name, array $args) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__call() cannot be static in %s on line 4
