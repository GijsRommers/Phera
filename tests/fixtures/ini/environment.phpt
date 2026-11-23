--TEST--
INI interpolation reads the request environment and nested fallbacks
--FILE--
<?php
putenv('PHERA_INI_FIXTURE=active');
$input = <<<'INI'
a="${PHERA_INI_FIXTURE}"
b=${PHERA_INI_MISSING:-${PHERA_INI_FIXTURE}}
INI;
var_dump(parse_ini_string($input));
--EXPECT--
array(2) {
  ["a"]=>
  string(6) "active"
  ["b"]=>
  string(6) "active"
}
