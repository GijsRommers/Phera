--TEST--
INI sections retain root keys and group arrays
--FILE--
<?php
$input = "root=first\n[web]\nport=80\n[db]\nport=3306";
var_dump(parse_ini_string($input, true));
--EXPECT--
array(3) {
  ["root"]=>
  string(5) "first"
  ["web"]=>
  array(1) {
    ["port"]=>
    string(2) "80"
  }
  ["db"]=>
  array(1) {
    ["port"]=>
    string(4) "3306"
  }
}
