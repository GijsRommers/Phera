--TEST--
A private clone method cannot be used from global scope
--FILE--
<?php
class Copy { private function __clone() {} }
try { clone new Copy; } catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Call to private method Copy::__clone() from global scope
