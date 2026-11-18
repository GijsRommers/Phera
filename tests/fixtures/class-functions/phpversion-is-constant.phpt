--TEST--
Phpversion is constant
--FILE--
<?php
var_dump(phpversion(),phpversion()===PHP_VERSION);
--EXPECT--
string(6) "8.5.12"
bool(true)
