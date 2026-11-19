--TEST--
INI module lists retain access, local values, and null defaults
--FILE--
<?php
ini_set('date.timezone', 'Europe/Brussels');
var_dump(ini_get_all('date')['date.timezone']);
var_dump(ini_get_all('date', false)['date.timezone']);
var_dump(ini_get_all('standard')['user_agent']);
var_dump(ini_get_all('ctype'), ini_get_all('pdo', false));
var_dump(@ini_get_all('Date'), @ini_get_all('missing'));
--EXPECT--
array(3) {
  ["global_value"]=>
  string(3) "UTC"
  ["local_value"]=>
  string(15) "Europe/Brussels"
  ["access"]=>
  int(7)
}
string(15) "Europe/Brussels"
array(3) {
  ["global_value"]=>
  NULL
  ["local_value"]=>
  NULL
  ["access"]=>
  int(7)
}
array(0) {
}
array(0) {
}
bool(false)
bool(false)
