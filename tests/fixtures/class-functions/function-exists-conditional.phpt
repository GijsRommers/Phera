--TEST--
Function exists conditional
--FILE--
<?php
var_dump(function_exists("later")); if(true) { function later() {} } var_dump(function_exists("later"));
--EXPECT--
bool(false)
bool(true)
