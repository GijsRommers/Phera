--TEST--
__callStatic declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public static function __callStatic(string $name, array $args, $extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__callStatic() must take exactly 2 arguments in %s on line 4
