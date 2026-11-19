--TEST--
INI files use the streams path reader
--FILE--
<?php
var_dump(parse_ini_file(__DIR__.'/example.inc', true, INI_SCANNER_TYPED));
--EXPECT--
array(1) {
  ["app"]=>
  array(2) {
    ["active"]=>
    bool(true)
    ["port"]=>
    int(8080)
  }
}
