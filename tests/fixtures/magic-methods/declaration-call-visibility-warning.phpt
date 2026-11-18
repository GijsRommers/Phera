--TEST--
__call declaration checks visibility warning
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    protected function __call(string $name, array $args) {}
}
--EXPECTF--
Warning: The magic method MagicDeclaration::__call() must have public visibility in %s on line 4
executed
