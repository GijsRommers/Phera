--TEST--
Exceptions: trace_string_limit
--FILE--
<?php
function capture($text) { return new Exception("length"); }
$failure = capture("abcdefghijklmnopqrst");
echo $failure->getTraceAsString(), "\n";
var_dump(ini_set("zend.exception_string_param_max_len", "5"));
echo $failure->getTraceAsString(), "\n";
var_dump(ini_set("zend.exception_string_param_max_len", "-1"));
var_dump(ini_set("zend.exception_string_param_max_len", "1000001"));
var_dump(ini_get("zend.exception_string_param_max_len"));
--EXPECTF--
#0 %s(3): capture('abcdefghijklmno...')
#1 {main}
string(2) "15"
#0 %s(3): capture('abcde...')
#1 {main}
bool(false)
bool(false)
string(1) "5"
