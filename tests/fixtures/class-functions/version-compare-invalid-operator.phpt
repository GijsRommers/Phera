--TEST--
Version compare invalid operator
--FILE--
<?php
try { version_compare("1","2","LT"); } catch(ValueError $e) { echo $e->getMessage(); }
--EXPECT--
version_compare(): Argument #3 ($operator) must be a valid comparison operator