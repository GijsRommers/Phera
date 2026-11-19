--TEST--
Quoted INI values keep newlines and decode Zend escapes
--FILE--
<?php
$input = <<<'INI'
a="line
two"
b="a\"b\$c\\d"
c='raw; value'
INI;
var_dump(parse_ini_string($input));
--EXPECT--
array(3) {
  ["a"]=>
  string(8) "line
two"
  ["b"]=>
  string(7) "a"b$c\d"
  ["c"]=>
  string(10) "raw; value"
}
