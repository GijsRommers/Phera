--TEST--
Phpversion is constant
--FILE--
<?php
var_dump(phpversion(),phpversion()===PHP_VERSION);
--EXPECT--
string(8) "8.6.0RC7"
bool(true)
