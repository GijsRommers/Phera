--TEST--
Disable limit
--INI--
max_execution_time=1
--FILE--
<?php
var_dump(set_time_limit(0));
echo ini_get('max_execution_time'), "\n";
ini_restore('max_execution_time');
echo ini_get('max_execution_time'), "\n";
--EXPECTF--
bool(true)
0
1
