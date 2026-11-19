--TEST--
Native named defaults: named function
--FILE--
<?php
var_dump(json_decode('{"a":1}', flags: JSON_OBJECT_AS_ARRAY));
var_dump(json_encode([[1]], depth: 3));
var_dump(str_pad('a', 5, pad_type: STR_PAD_LEFT));
var_dump(number_format(1234.567, thousands_separator: ' '));
var_dump(preg_match('/b/', 'abc', offset: 1));
var_dump(preg_split('/,/', 'a,b,,c', flags: PREG_SPLIT_NO_EMPTY));
var_dump(htmlspecialchars('<"', double_encode: false));
?>
--EXPECT--
array(1) {
  ["a"]=>
  int(1)
}
string(5) "[[1]]"
string(5) "    a"
string(5) "1 235"
int(1)
array(3) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "b"
  [2]=>
  string(1) "c"
}
string(10) "&lt;&quot;"
