--TEST--
INI constants resolve in the current request
--SKIPIF--
<?php if (PHP_VERSION_ID < 80513) die('skip PHP behaviour since 8.5.13'); ?>
--FILE--
<?php
const PROJECT_VALUE = 19;
var_dump(parse_ini_string("mask=E_ALL & ~E_NOTICE\nversion=PHP_VERSION\nnum=PROJECT_VALUE"));
--EXPECT--
array(3) {
  ["mask"]=>
  string(5) "30711"
  ["version"]=>
  string(6) "8.5.13"
  ["num"]=>
  string(2) "19"
}
