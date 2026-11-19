--TEST--
INI constants resolve in the current request
--FILE--
<?php
const PROJECT_VALUE = 19;
var_dump(parse_ini_string("mask=E_ALL & ~E_NOTICE\nversion=PHP_VERSION\nnum=PROJECT_VALUE"));
--EXPECT--
array(3) {
  ["mask"]=>
  string(5) "30711"
  ["version"]=>
  string(6) "8.5.12"
  ["num"]=>
  string(2) "19"
}
