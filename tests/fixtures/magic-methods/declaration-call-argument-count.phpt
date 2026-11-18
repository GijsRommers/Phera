--TEST--
__call declaration checks argument count
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __call(string $name, array $args, $extra) {}
}
--EXPECTF--
Fatal error: Method MagicDeclaration::__call() must take exactly 2 arguments in %s on line 4
