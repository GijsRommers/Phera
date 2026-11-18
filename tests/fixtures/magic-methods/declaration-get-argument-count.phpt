--TEST--
__get declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __get(string $name, $extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__get() must take exactly 1 argument in %s on line 4
