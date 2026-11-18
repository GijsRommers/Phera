--TEST--
A real public nonstatic method remains an error when called statically
--FILE--
<?php
class A { function run() {} static function __callStatic($n, $a) { echo "magic"; } }
try { A::run(); } catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Non-static method A::run() cannot be called statically
