--TEST--
__isset declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __isset(string $name, $extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__isset() must take exactly 1 argument in %s on line 4
