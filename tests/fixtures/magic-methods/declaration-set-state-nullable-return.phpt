--TEST--
__set_state declaration checks nullable return
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __set_state(array $properties): ?object {}
}
--EXPECTF--
Fatal error: MagicDeclaration::__set_state(): Return type must be object when declared in %s on line 4
