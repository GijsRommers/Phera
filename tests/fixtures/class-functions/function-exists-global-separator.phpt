--TEST--
Function exists global separator
--FILE--
<?php
var_dump(function_exists("\\STrLen"));
--EXPECT--
bool(true)
