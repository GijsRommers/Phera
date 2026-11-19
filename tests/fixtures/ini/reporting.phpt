--TEST--
INI error reporting and error_reporting share the request mask
--FILE--
<?php
error_reporting(17);
var_dump(ini_get('error_reporting'), ini_set('error_reporting', '3'), error_reporting());
error_reporting(9);
var_dump(ini_get('error_reporting'), ini_get_all()['error_reporting']);
ini_restore('error_reporting');
var_dump(ini_get('error_reporting'));
--EXPECT--
string(2) "17"
string(2) "17"
int(3)
string(1) "9"
array(3) {
  ["global_value"]=>
  string(1) "9"
  ["local_value"]=>
  string(1) "9"
  ["access"]=>
  int(7)
}
string(0) ""
