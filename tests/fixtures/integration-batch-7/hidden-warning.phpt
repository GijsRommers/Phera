--TEST--
display_errors hides a warning without clearing the last error
--FILE--
<?php
ini_set('display_errors', '0');
trigger_error('hidden warning', E_USER_WARNING);
$error = error_get_last();
var_dump($error['type'], $error['message'], $error['line']);
--EXPECT--
int(512)
string(14) "hidden warning"
int(3)
