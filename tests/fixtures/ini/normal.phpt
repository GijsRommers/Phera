--TEST--
Normal INI mode produces strings for special words
--FILE--
<?php
var_dump(parse_ini_string("a=YES\nb=off\nc=null\nd=1\ne=\n"));
--EXPECT--
array(5) {
  ["a"]=>
  string(1) "1"
  ["b"]=>
  string(0) ""
  ["c"]=>
  string(0) ""
  ["d"]=>
  string(1) "1"
  ["e"]=>
  string(0) ""
}
