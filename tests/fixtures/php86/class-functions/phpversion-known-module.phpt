--TEST--
Phpversion known module
--FILE--
<?php
var_dump(phpversion("PDO"));
--EXPECT--
string(8) "8.6.0RC7"
