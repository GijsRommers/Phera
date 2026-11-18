--TEST--
__get declaration checks covariant return
--FILE--
<?php
echo "executed\n";
class MagicDeclaration {
    public function __get($name): int {}
}
--EXPECT--
executed
