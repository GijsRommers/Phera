--TEST--
A protected clone method cannot be used from global scope
--FILE--
<?php
class Copy { protected function __clone() {} }
try { clone new Copy; } catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Call to protected method Copy::__clone() from global scope
