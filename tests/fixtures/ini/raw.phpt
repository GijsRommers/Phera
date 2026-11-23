--TEST--
Raw INI values preserve literals and discard comments
--FILE--
<?php
$input = <<<'INI'
[ raw ]
mask=E_ALL & ~E_NOTICE
boolean=yes
value="a;b";comment
single='literal'
INI;
var_dump(parse_ini_string($input, true, INI_SCANNER_RAW));
--EXPECT--
array(1) {
  [" raw "]=>
  array(4) {
    ["mask"]=>
    string(17) "E_ALL & ~E_NOTICE"
    ["boolean"]=>
    string(3) "yes"
    ["value"]=>
    string(3) "a;b"
    ["single"]=>
    string(9) "'literal'"
  }
}
