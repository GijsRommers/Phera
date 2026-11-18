--TEST--
Version compare patch order
--FILE--
<?php
var_dump(version_compare("8.5.12-pl2","8.5.12","gt"));
--EXPECT--
bool(true)
