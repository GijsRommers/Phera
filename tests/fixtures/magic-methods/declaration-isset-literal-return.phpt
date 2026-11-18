--TEST--
__isset declaration checks literal return
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __isset($name): false {}
}
--EXPECT--
executed
