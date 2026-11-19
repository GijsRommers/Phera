--TEST--
Phpversion known module
--FILE--
<?php
var_dump(phpversion("PDO"));
--EXPECT--
string(6) "8.5.12"
